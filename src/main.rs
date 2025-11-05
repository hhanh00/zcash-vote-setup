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
    Ok(())
}
