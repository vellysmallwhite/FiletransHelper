use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TransportPing {
    pub device_id: String,
    pub device_name: String,
    pub version: String,
    pub platform: String,
    pub protocol_version: u16,
    pub features: Vec<String>,
}

pub trait Transport: Send + Sync {
    #[allow(dead_code)]
    fn name(&self) -> &'static str;
}

pub mod http;
pub mod quic;
pub mod ws_client;
