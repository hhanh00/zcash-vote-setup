use std::{
    fs::{self, File},
    io::Write,
    process::Command,
};

use anyhow::Result;
use figment::{
    Figment,
    providers::{Format, Yaml},
};
use serde_json::Value;
use toml_edit::DocumentMut;

use crate::config::Config;

pub mod config;

pub const VERSION: &str = "1.2.1";

fn main() -> Result<()> {
    let config: Config = Figment::new().merge(Yaml::file("config.yml")).extract()?;
    println!("{config:?}");
    let uid = config.uid;

    let _ = std::fs::create_dir_all("__tmp/data");
    let _ = std::fs::create_dir_all("__tmp/db");

    // Copy election files
    Command::new("/bin/bash")
        .args(shell_words::split(&format!(
            r#"-c "cp {}/*.json __tmp/data""#,
            &config.datadir
        ))?)
        .status()?;

    let args = format!("run --rm -w /home/user -u {uid} -e HOME=/home/user -v ./__tmp:/home/user --entrypoint /bin/bash hhanh00/zcash-vote-docker:{VERSION} -c 'cp /Rocket.toml .;/zcash-vote-server/zcash-vote-server -q'");
    Command::new("docker")
        .args(shell_words::split(&args)?)
        .status()?;

    create_cometbft_config("__tmp", uid)?;

    let mut addresses = vec![];
    let mut validators = vec![];
    for n in config.nodes.iter() {
        let (id, validator) = create_cometbft_config(&n.name, uid)?;
        let node_address = format!("{id}@{}:26656", n.name);
        addresses.push(node_address);
        validators.push(validator);
    }

    // Update main genesis.json
    let genesis_filename = "__tmp/.cometbft/config/genesis.json";
    let mut genesis = serde_json::from_reader::<_, Value>(&File::open(genesis_filename)?)?;
    let node_validators = genesis["validators"].as_array_mut().unwrap();
    *node_validators = validators.to_vec();
    serde_json::to_writer_pretty(&File::create(genesis_filename)?, &genesis)?;

    for (i, n) in config.nodes.iter().enumerate() {
        let dir = &n.name;
        let auth = &config.ts_authkey;
        Command::new("cp")
            .args(shell_words::split(&format!("__tmp/.cometbft/config/genesis.json {}/.cometbft/config/", &n.name))?)
            .status()?;

        update_cometbft_config(i, dir, &addresses)?;

        let args = format!("-c 'cp __tmp/db/vote.db {dir}/db/'");
        Command::new("/bin/bash")
            .args(shell_words::split(&args)?)
            .status()?;

        let args = format!("-c 'cp __tmp/data/* {dir}/data/'");
        Command::new("/bin/bash")
            .args(shell_words::split(&args)?)
            .status()?;

        let mut run_script = File::create(format!("{dir}/run.sh"))?;
        let port_mapping = if let Some(p) = n.port {
            format!("-p {p}:8000")
        } else {
            "".to_string()
        };
        writeln!(run_script, "docker run --privileged --name {dir} -it {port_mapping} -v .:/home/user -v ./tailscale-data:/var/lib/tailscale -e NODE={dir} -e TS_AUTHKEY={auth} hhanh00/zcash-vote-docker:{VERSION}")?;
    }

    Ok(())
}

pub fn create_cometbft_config(dir: &str, uid: u32) -> Result<(String, Value)> {
    let _ = std::fs::create_dir_all(format!("{dir}/.cometbft"));
    let _ = std::fs::create_dir_all(format!("{dir}/data"));
    let _ = std::fs::create_dir_all(format!("{dir}/db"));

    let args = format!("run --rm -w /home/user -e HOME=/home/user -u {uid} -v ./.cometbft:/home/user/.cometbft --entrypoint /bin/bash hhanh00/zcash-vote-docker:{VERSION} -c 'cometbft init >/dev/null;cometbft show-node-id'");
    let output = Command::new("docker")
        .current_dir(dir)
        .args(shell_words::split(&args)?)
        .output()?;

    let node_id = str::from_utf8(&output.stdout)?.trim_end();
    println!("{node_id}");

    let genesis = serde_json::from_reader::<_, Value>(&File::open(format!(
        "{dir}/.cometbft/config/genesis.json"
    ))?)?;
    let validator = &genesis["validators"].as_array().unwrap()[0];
    Ok((node_id.to_string(), validator.clone()))
}

pub fn update_cometbft_config(
    i: usize,
    dir: &str,
    addresses: &[String],
) -> Result<()> {
    let peers = addresses
        .iter()
        .enumerate()
        .filter_map(|(idx, a)| if idx != i { Some(a.clone()) } else { None })
        .collect::<Vec<String>>()
        .join(",");

    let config_filename = format!("{dir}/.cometbft/config/config.toml");
    let config = fs::read_to_string(&config_filename)?;
    let mut config = config.parse::<DocumentMut>()?;
    config["p2p"]["persistent_peers"] = peers.into();
    config["rpc"]["timeout_broadcast_tx_commit"] = "60s".into();
    let mut config_file = File::create(&config_filename)?;
    config_file.write_all(config.to_string().as_bytes())?;

    Ok(())
}
