use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug)]
pub struct Node {
    pub name: String,
    pub port: Option<u16>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Config {
    pub ts_authkey: String,
    pub uid: u32,
    pub nodes: Vec<Node>,
    pub datadir: String,
}
