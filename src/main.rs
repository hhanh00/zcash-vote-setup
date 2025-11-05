use std::process::Command;

use anyhow::Result;
use figment::{
    Figment,
    providers::{Format, Yaml},
};

use crate::config::Config;

pub mod config;

fn main() -> Result<()> {
    let config: Config = Figment::new().merge(Yaml::file("config.yml")).extract()?;
    println!("{config:?}");

    let _ = std::fs::create_dir_all("__tmp/data");
    let _ = std::fs::create_dir_all("__tmp/db");

    Command::new("/bin/bash")
    .args(shell_words::split(&format!(r#"-c "cp {}/*.json __tmp/data""#, &config.datadir))?)
    .spawn()?
    .wait()?;

    Command::new("touch")
    .args(shell_words::split("__tmp/db/vote.db")?)
    .spawn()?
    .wait()?;


    let args = "run --rm -w /root/zcash-vote-server -v ./data:/root/zcash-vote-server/data -v ./db:/root/zcash-vote-server/db --entrypoint /root/zcash-vote-server/zcash-vote-server hhanh00/zcash-vote-docker:1.2.1 -q";
    Command::new("docker")
    .current_dir("__tmp")
    .args(shell_words::split(args)?)
    .spawn()?
    .wait()?;



    Ok(())
}
