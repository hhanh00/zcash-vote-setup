use std::env;
use std::fs::{File, create_dir_all, exists};
use std::io::Write;
use std::process::Command;
use std::time::Duration;

use anyhow::Result;
use clap::Parser;
use serde_json::Value;
use sqlx::sqlite::SqliteConnectOptions;
use sqlx::{Connection, SqliteConnection};
use tonic::{Request, transport::Channel};
use tracing::info;
use zcash_vote_setup::Validator;
use zcash_vote_setup::rpc::{AuthReq, NodeDef, TsAuthKey};
use zcash_vote_setup::util::run_command_in_container;

pub type Client = zcash_vote_setup::rpc::vote_server_setup_client::VoteServerSetupClient<Channel>;

#[derive(Parser, Debug)]
pub struct Config {
    #[clap(short, long)]
    url: String,
    #[clap(short, long)]
    password: String,
    #[clap(short, long)]
    force: Option<bool>,
}

#[tokio::main]
pub async fn main() -> Result<()> {
    rustls::crypto::aws_lc_rs::default_provider()
        .install_default()
        .unwrap();
    let subscriber = tracing_subscriber::fmt()
        .with_ansi(false)
        .compact()
        .finish();
    let _ = tracing::subscriber::set_global_default(subscriber);

    let config = Config::parse();
    let Config {
        url,
        password,
        force,
    } = config;
    let force = force.unwrap_or_default();
    let uid = users::get_current_uid();
    let username = users::get_current_username()
        .unwrap()
        .to_string_lossy()
        .to_string();
    let channel = Channel::from_shared(url)?;
    let mut client = Client::connect(channel).await?;
    let TsAuthKey { name, key } = get_authkey(&mut client, &password).await?;

    if exists(&name)? && !force {
        anyhow::bail!("Node directory already exists. Use -f to force using the existing setup.");
    }

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
            &username,
            &key,
            "tailscaled & sleep 5; tailscale up --auth-key=$TS_AUTHKEY --hostname=$NODE",
        )?;
    }
    if !exists("home/.cometbft/config/node_key.json")? {
        info!("Initializing new cometbft node");
        run_command_in_container(&name, uid, &username, &key, "cometbft init")?;
    }

    loop {
        let node_id =
            run_command_in_container(&name, uid, &username, &key, "cometbft show-node-id")?;

        let genesis_file = File::open("home/.cometbft/config/genesis.json")?;
        let genesis: Value = serde_json::from_reader(genesis_file)?;
        let validators: Vec<Validator> = serde_json::from_value(genesis["validators"].clone())?;
        if validators.len() == 1 {
            let validator = &validators[0];

            let node = NodeDef {
                password: password.clone(),
                pubkey: validator.pub_key.value.clone(),
                address: validator.address.clone(),
                id: node_id,
            };

            info!("Node config: {:?}", &node);
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
            } else {
                println!("{} more nodes to add.", config.remaining);
            }
            std::thread::sleep(Duration::from_secs(5));
        }
        else {
            break;
        }
    }

    // run.sh script
    let status = Command::new("sh")
        .arg("run.sh")
        .status()
        .expect("failed to execute run script");

    tracing::info!("{}", status);

    Ok(())
}

pub async fn get_authkey(client: &mut Client, password: &str) -> Result<TsAuthKey> {
    let rep = client
        .get_ts_auth_key(Request::new(AuthReq {
            password: password.to_string(),
        }))
        .await?
        .into_inner();
    Ok(rep)
}
