use std::{
    collections::HashSet,
    env,
    fs::{self, File, create_dir_all},
    io::Read,
    net::{IpAddr, Ipv4Addr, SocketAddr},
    path::{self, PathBuf},
    process::Command,
    str::FromStr,
};

use anyhow::Result;
use figment::{
    Figment,
    providers::{Format, Yaml},
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::{
    Row, SqliteConnection, SqlitePool, query,
    sqlite::{SqliteConnectOptions, SqliteRow},
};
use toml_edit::DocumentMut;
use tonic::{Request, Response, Status, transport::Server};
use tracing::info;
use zcash_vote_setup::{PubKey, VERSION, Validator, rpc::*, util::run_command_in_container};

#[derive(Serialize, Deserialize, Debug)]
pub struct ServerConfig {
    pub chainid: String,
    pub auth: String,
    pub datadir: String,
    pub workdir: String,
    pub peers: Vec<String>,
    pub port: u16,
}

struct VoteServerImpl {
    chainid: String,
    auth_key: String,
    peers: HashSet<String>,
    pool: SqlitePool,
}

#[tonic::async_trait]
impl zcash_vote_setup::rpc::vote_server_setup_server::VoteServerSetup for VoteServerImpl {
    async fn get_ts_auth_key(
        &self,
        _request: Request<Empty>,
    ) -> Result<Response<TsAuthKey>, Status> {
        let rep = Response::new(TsAuthKey {
            key: self.auth_key.clone(),
        });
        Ok(rep)
    }

    async fn put_node_def(
        &self,
        request: Request<NodeDef>,
    ) -> Result<Response<NodeConfig>, Status> {
        let run = async move {
            let mut connection = self.db_connect().await?;
            let node_def = request.into_inner();
            if !self.peers.contains(&node_def.name) {
                anyhow::bail!("Node is not part of the config");
            }

            let validator = Validator {
                address: node_def.address,
                pub_key: PubKey {
                    r#type: "tendermint/PubKeyEd25519".to_string(),
                    value: node_def.pubkey,
                },
                power: "10".to_string(),
                name: node_def.name.clone(),
            };
            query(
                "INSERT INTO nodes(name, id, validator)
            VALUES (?1, ?2, ?3) ON CONFLICT DO NOTHING",
            )
            .bind(&node_def.name)
            .bind(node_def.id.trim_end())
            .bind(serde_json::to_string(&validator).unwrap())
            .execute(&mut connection)
            .await?;

            let (count,): (u32,) = sqlx::query_as("SELECT COUNT(*) FROM nodes")
                .fetch_one(&mut connection)
                .await?;
            let n = if count == self.peers.len() as u32 {
                build_node_config(
                    &mut connection,
                    &node_def.name,
                    &self.auth_key,
                    &self.chainid,
                )
                .await?
            } else {
                NodeConfig {
                    remaining: self.peers.len() as u32 - count,
                    ..NodeConfig::default()
                }
            };
            Ok::<_, anyhow::Error>(n)
        };
        let n = run.await.map_err(|e| Status::internal(e.to_string()))?;
        Ok(Response::new(n))
    }
}

pub async fn build_node_config(
    connection: &mut SqliteConnection,
    name: &str,
    auth: &str,
    chainid: &str,
) -> Result<NodeConfig> {
    // vote.db
    let (votedb,): (Vec<u8>,) = sqlx::query_as("SELECT data FROM votedb")
        .fetch_one(&mut *connection)
        .await?;

    // genesis.json
    let validators = query("SELECT validator FROM nodes ORDER BY name")
        .map(|r: SqliteRow| {
            let v: String = r.get(0);
            serde_json::from_str::<Value>(&v).unwrap()
        })
        .fetch_all(&mut *connection)
        .await?;

    let genesis = include_str!("tmpl/genesis.json");
    let mut genesis = serde_json::from_str::<Value>(genesis)?;
    genesis["chain_id"] = chainid.into();
    let node_validators = genesis["validators"].as_array_mut().unwrap();
    *node_validators = validators.to_vec();
    let genesis = serde_json::to_string_pretty(&genesis)?;

    // config.toml
    let node_ids: Vec<(String, String)> =
        sqlx::query_as("SELECT name, id FROM nodes WHERE name <> ?1 ORDER BY name")
            .bind(name)
            .fetch_all(&mut *connection)
            .await?;

    let peers = node_ids
        .iter()
        .map(|(name, id)| format!("{id}@{name}:26656"))
        .collect::<Vec<String>>()
        .join(",");

    let config = include_str!("tmpl/config.toml");
    let mut config = config.parse::<DocumentMut>()?;
    config["moniker"] = name.into();
    config["p2p"]["persistent_peers"] = peers.into();
    config["rpc"]["timeout_broadcast_tx_commit"] = "60s".into();
    let config = config.to_string();

    // run.sh
    let run = format!(
        "docker run -d --privileged --name {name} -it -v ./home:/home/user -v ./tailscale-data:/var/lib/tailscale -e NODE={name} -e TS_AUTHKEY={auth} hhanh00/zcash-vote-docker:{VERSION}"
    );

    let n = NodeConfig {
        config,
        votedb,
        genesis,
        run,
        remaining: 0,
    };
    Ok(n)
}

impl VoteServerImpl {
    pub async fn db_connect(&self) -> Result<SqliteConnection> {
        let pool = self.pool.acquire().await?;
        let connection = pool.detach();
        Ok(connection)
    }
}

#[tokio::main]
pub async fn main() -> Result<()> {
    let subscriber = tracing_subscriber::fmt()
        .with_ansi(false)
        .compact()
        .finish();
    let _ = tracing::subscriber::set_global_default(subscriber);
    let uid = users::get_current_uid();
    let username = users::get_current_username().unwrap().to_string_lossy().to_string();
    let config: ServerConfig = Figment::new()
        .merge(Yaml::file("server_config.yml"))
        .extract()?;
    fs::create_dir_all(&config.workdir)?;

    let db_filename = format!("{}/setup.db", &config.workdir);
    let db_filename = PathBuf::from_str(&db_filename)?;
    let db_filename = path::absolute(db_filename)?;
    let db_options = SqliteConnectOptions::new()
        .filename(db_filename)
        .create_if_missing(true);
    let db = SqlitePool::connect_with(db_options).await?;
    let mut connection = db.acquire().await?;
    create_schema(&mut connection).await?;

    create_dir_all(format!("{}/home/data", config.workdir))?;

    Command::new("/bin/bash")
        .args(&shell_words::split(&format!(
            "-c 'cp {}/* {}/home/data'",
            config.datadir, config.workdir
        ))?)
        .status()?;

    env::set_current_dir(config.workdir)?;
    create_dir_all("tailscale-data")?;
    create_dir_all("home/db")?;

    let is_imported = query("SELECT 1 FROM votedb WHERE id = 0")
        .fetch_optional(&mut *connection)
        .await?
        .is_some();

    // Store the content of vote.db in setup.db
    if !is_imported {
        // import the election data into the vote.db
        let r = run_command_in_container("", uid, &username, "", "/zcash-vote-server/zcash-vote-server -q")?;
        println!("{r}");

        let mut vote_db_file = File::open("home/db/vote.db")?;
        let mut vote_db_bin = vec![];
        vote_db_file.read_to_end(&mut vote_db_bin)?;
        query("INSERT INTO votedb(id, data) VALUES (0, ?1)")
            .bind(&vote_db_bin)
            .execute(&mut *connection)
            .await?;
    };

    let addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), config.port);
    let handler = VoteServerImpl {
        chainid: config.chainid,
        auth_key: config.auth,
        peers: config.peers.into_iter().collect(),
        pool: db,
    };
    let server =
        zcash_vote_setup::rpc::vote_server_setup_server::VoteServerSetupServer::new(handler);
    info!("Listening at {addr}");
    Server::builder().add_service(server).serve(addr).await?;

    Ok(())
}

pub async fn create_schema(connection: &mut SqliteConnection) -> Result<()> {
    query(
        "CREATE TABLE IF NOT EXISTS votedb(
        id INTEGER PRIMARY KEY,
        data BLOB NOT NULL)",
    )
    .execute(&mut *connection)
    .await?;
    query(
        "CREATE TABLE IF NOT EXISTS nodes(
        name TEXT PRIMARY KEY,
        id TEXT NOT NULL,
        validator TEXT NOT NULL)",
    )
    .execute(&mut *connection)
    .await?;
    Ok(())
}
