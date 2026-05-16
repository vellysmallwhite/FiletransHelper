use std::time::{SystemTime, UNIX_EPOCH};

use serde::Serialize;
use url::Url;

use crate::store::sqlite::SqliteStore;
use crate::transport::http::HttpTransport;

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Device {
    pub id: String,
    pub name: String,
    pub ip: String,
    pub port: u16,
    pub public_key: Option<String>,
    pub trusted: bool,
    pub platform: Option<String>,
    pub version: Option<String>,
    pub protocol_version: Option<i64>,
    pub features: Vec<String>,
    pub online: bool,
    pub created_at: i64,
    pub last_seen_at: Option<i64>,
    pub last_checked_at: Option<i64>,
    pub last_error: Option<String>,
}

#[derive(Clone, Debug)]
pub struct DeviceEndpoint {
    pub host: String,
    pub port: u16,
}

#[derive(Clone, Debug)]
pub struct DeviceManager {
    store: SqliteStore,
    transport: HttpTransport,
}

impl DeviceManager {
    pub fn new(store: SqliteStore, transport: HttpTransport) -> Self {
        Self { store, transport }
    }

    pub fn list_devices(&self) -> Result<Vec<Device>, String> {
        self.store.list_devices()
    }

    pub async fn add_device(&self, address: &str) -> Result<Device, String> {
        let endpoint = parse_endpoint(address)?;
        let ping = self.transport.ping(&endpoint).await?;
        self.store
            .upsert_online_device(&endpoint, &ping, unix_millis())
    }

    pub async fn refresh_device_status(&self) -> Result<Vec<Device>, String> {
        let devices = self.store.list_devices()?;

        for device in devices {
            let endpoint = DeviceEndpoint {
                host: device.ip.clone(),
                port: device.port,
            };
            let checked_at = unix_millis();

            match self.transport.ping(&endpoint).await {
                Ok(ping) => {
                    self.store
                        .upsert_online_device(&endpoint, &ping, checked_at)?;
                }
                Err(err) => {
                    self.store
                        .mark_device_offline(&device.id, checked_at, err)?;
                }
            }
        }

        self.store.list_devices()
    }
}

fn parse_endpoint(address: &str) -> Result<DeviceEndpoint, String> {
    let trimmed = address.trim();
    if trimmed.is_empty() {
        return Err("请输入设备地址，例如 PEER_ZEROTIER_IP:8765".to_string());
    }

    let raw_url = if trimmed.contains("://") {
        trimmed.to_string()
    } else {
        format!("http://{trimmed}")
    };

    let url = Url::parse(&raw_url).map_err(|_| "设备地址格式不正确，请使用 ip:port".to_string())?;
    if url.scheme() != "http" {
        return Err("Phase 2 仅支持 HTTP 地址".to_string());
    }

    let host = url
        .host_str()
        .map(str::to_string)
        .ok_or_else(|| "设备地址缺少 IP 或主机名".to_string())?;
    let port = url.port().unwrap_or(8765);

    Ok(DeviceEndpoint { host, port })
}

fn unix_millis() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| i64::try_from(duration.as_millis()).unwrap_or(i64::MAX))
        .unwrap_or_default()
}
