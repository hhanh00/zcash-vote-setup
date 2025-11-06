use std::env;
use std::fs::{File, create_dir_all, exists};
use std::io::Write;

use anyhow::Result;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::sqlite::SqliteConnectOptions;
use sqlx::{Connection, SqliteConnection};
use tonic::{Request, transport::Channel};
use zcash_vote_setup::Validator;
use zcash_vote_setup::rpc::{Empty, NodeDef, TsAuthKey};
use zcash_vote_setup::util::run_command_in_container;

pub type Client = zcash_vote_setup::rpc::vote_server_setup_client::VoteServerSetupClient<Channel>;

#[derive(Serialize, Deserialize, Debug)]
pub struct ClientConfig {
    pub name: String,
}

#[tokio::main]
pub async fn main() -> Result<()> {
    let args: Vec<String> = env::args().collect();
    if args.len() != 3 {
        anyhow::bail!("<server url> <node name>");
    }
    let url: String = args[1].clone();
    let name = args[2].clone();
    let uid = users::get_current_uid();
    let channel = Channel::from_shared(url)?;
    let mut client = Client::connect(channel).await?;
    let auth_key = get_authkey(&mut client).await?.key;

    create_dir_all(&name)?;
    env::set_current_dir(&name)?;
    create_dir_all("tailscale-data")?;
    create_dir_all("home/data")?;
    create_dir_all("home/db")?;

    if !exists("tailscale-data/tailscaled.state")? {
        // Connect to tailscale
        // User must be root (in the container)
        run_command_in_container(
            &name,
            0,
            &auth_key,
            "tailscaled & sleep 5; tailscale up --auth-key=$TS_AUTHKEY --hostname=$NODE",
        )?;
    }
    if !exists(".cometbft/config/node_key.json")? {
        run_command_in_container(&name, uid, &auth_key, "cometbft init")?;
    }
    let node_id = run_command_in_container(&name, uid, &auth_key, "cometbft show-node-id")?;

    let genesis_file = File::open("home/.cometbft/config/genesis.json")?;
    let genesis: Value = serde_json::from_reader(genesis_file)?;
    let validators: Vec<Validator> = serde_json::from_value(genesis["validators"].clone())?;
    if validators.len() == 1 {
        let validator = &validators[0];

        let node = NodeDef {
            name: name.clone(),
            pubkey: validator.pub_key.value.clone(),
            address: validator.address.clone(),
            id: node_id,
        };

        let config = client.put_node_def(Request::new(node)).await?.into_inner();
        if config.remaining == 0 {
            let mut votedb_file = File::create("home/db/vote.db")?;
            votedb_file.write_all(&config.votedb)?;
            let mut config_file = File::create("home/.cometbft/config/config.toml")?;
            writeln!(config_file, "{}", config.config)?;
            let mut genesis_file = File::create("home/.cometbft/config/genesis.json")?;
            writeln!(genesis_file, "{}", config.genesis)?;
            let mut run_file = File::create("run.sh")?;
            writeln!(run_file, "{}", config.run)?;

            let options = SqliteConnectOptions::new().filename("home/db/vote.db");
            let mut connection = SqliteConnection::connect_with(&options).await?;
            let elections: Vec<(String, String)> =
                sqlx::query_as("SELECT id, definition FROM elections")
                    .fetch_all(&mut connection)
                    .await?;
            for (id, definition) in elections {
                let mut e = File::create(format!("home/data/{id}.json"))?;
                writeln!(e, "{}", definition)?;
            }
            println!("Configuration written.");
        }
        else {
            println!("{} more nodes to add.", config.remaining);
        }
    }

    Ok(())
}

pub async fn get_authkey(client: &mut Client) -> Result<TsAuthKey> {
    let rep = client
        .get_ts_auth_key(Request::new(Empty {}))
        .await?
        .into_inner();
    Ok(rep)
}
