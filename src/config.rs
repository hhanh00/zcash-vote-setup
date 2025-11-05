use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug)]
#[allow(non_camel_case_types)]
pub enum Arch {
    x86_64, aarch64,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Node {
    pub name: String,
    pub arch: Arch,
    pub port: Option<u16>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Config {
    pub ts_authkey: String,
    pub arch: Arch,
    pub nodes: Vec<Node>,
    pub datadir: String,
}
