use serde::{Deserialize, Serialize};

pub mod rpc {
    tonic::include_proto!("vote_setup.rpc");
}

pub mod util;

pub const VERSION: &str = "1.2.1";

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct PubKey {
    pub r#type: String,
    pub value: String,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct Validator {
    pub address: String,
    pub pub_key: PubKey,
    pub power: String,
    pub name: String,
}
