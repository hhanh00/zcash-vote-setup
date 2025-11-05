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
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Config {
    ts_authkey: String,
    arch: Arch,
    nodes: Vec<Node>,
}
