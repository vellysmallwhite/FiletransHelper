use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::Serialize;
use tauri::AppHandle;
use tokio::sync::RwLock;

use crate::config::{self, AppConfig};
use crate::device::manager::DeviceManager;
use crate::message::manager::MessageManager;
use crate::server::ws::{WsConnectionInfo, WsHub};
use crate::store::sqlite::SqliteStore;
use crate::transfer::manager::TransferManager;
use crate::transport::http::HttpTransport;
use crate::transport::ws_client::WsClientManager;

#[derive(Clone)]
pub struct AppState {
    inner: Arc<AppStateInner>,
}

struct AppStateInner {
    config: AppConfig,
    config_path: String,
    store: SqliteStore,
    device_manager: DeviceManager,
    message_manager: MessageManager,
    transfer_manager: TransferManager,
    ws_hub: WsHub,
    ws_client_manager: WsClientManager,
    app_handle: RwLock<Option<AppHandle>>,
    server_status: RwLock<ServerStatus>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerStatus {
    pub state: ServerState,
    pub bind_addr: String,
    pub error: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ServerState {
    Stopped,
    Starting,
    Running,
    Failed,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalInfo {
    pub device_id: String,
    pub device_name: String,
    pub version: String,
    pub platform: String,
    pub protocol_version: u16,
    pub listen_host: String,
    pub listen_port: u16,
    pub download_dir: String,
    pub config_path: String,
    pub database_path: String,
    pub features: Vec<String>,
    pub server_status: ServerStatus,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PingResponse {
    pub device_id: String,
    pub device_name: String,
    pub version: String,
    pub platform: String,
    pub protocol_version: u16,
    pub features: Vec<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentStatusEvent {
    pub timestamp: u128,
    pub local_info: LocalInfo,
}

impl AppState {
    pub fn load_or_init() -> Result<Self, String> {
        let loaded = config::load_or_init()?;
        let bind_addr = format!(
            "{}:{}",
            loaded.config.listen_host, loaded.config.listen_port
        );
        let ws_hub = WsHub::new(
            loaded.config.device_id.clone(),
            loaded.config.device_name.clone(),
        );
        let ws_client_manager = WsClientManager::new(ws_hub.clone());

        Ok(Self {
            inner: Arc::new(AppStateInner {
                message_manager: MessageManager::new(
                    loaded.store.clone(),
                    HttpTransport::default(),
                    loaded.config.device_id.clone(),
                    loaded.config.device_name.clone(),
                ),
                transfer_manager: TransferManager::new(
                    loaded.store.clone(),
                    HttpTransport::default(),
                    loaded.config.device_id.clone(),
                    loaded.config.device_name.clone(),
                    loaded.config.download_dir.clone(),
                ),
                config: loaded.config,
                config_path: loaded.config_path.display().to_string(),
                store: loaded.store.clone(),
                device_manager: DeviceManager::new(loaded.store, HttpTransport::default()),
                ws_hub,
                ws_client_manager,
                app_handle: RwLock::new(None),
                server_status: RwLock::new(ServerStatus {
                    state: ServerState::Stopped,
                    bind_addr,
                    error: None,
                }),
            }),
        })
    }

    pub fn bind_addr(&self) -> String {
        format!(
            "{}:{}",
            self.inner.config.listen_host, self.inner.config.listen_port
        )
    }

    pub fn device_manager(&self) -> &DeviceManager {
        &self.inner.device_manager
    }

    pub fn message_manager(&self) -> &MessageManager {
        &self.inner.message_manager
    }

    pub fn transfer_manager(&self) -> &TransferManager {
        &self.inner.transfer_manager
    }

    pub fn ws_hub(&self) -> &WsHub {
        &self.inner.ws_hub
    }

    pub async fn connect_device_ws(&self, peer_device_id: &str) -> Result<(), String> {
        let device = self
            .inner
            .store
            .get_device(peer_device_id)?
            .ok_or_else(|| "设备不存在，请先添加设备".to_string())?;
        self.inner
            .ws_client_manager
            .connect_device(device, self.clone())
            .await;
        Ok(())
    }

    pub async fn connect_all_device_ws(&self) -> Result<(), String> {
        self.inner.ws_client_manager.connect_all(self.clone()).await
    }

    pub async fn list_ws_connections(&self) -> Vec<WsConnectionInfo> {
        self.inner.ws_hub.list_connections().await
    }

    pub async fn set_app_handle(&self, app_handle: AppHandle) {
        let mut handle = self.inner.app_handle.write().await;
        *handle = Some(app_handle);
    }

    pub async fn app_handle(&self) -> Option<AppHandle> {
        self.inner.app_handle.read().await.clone()
    }

    pub async fn mark_server_starting(&self) {
        let mut status = self.inner.server_status.write().await;
        *status = ServerStatus {
            state: ServerState::Starting,
            bind_addr: self.bind_addr(),
            error: None,
        };
    }

    pub async fn mark_server_running(&self, bind_addr: String) {
        let mut status = self.inner.server_status.write().await;
        *status = ServerStatus {
            state: ServerState::Running,
            bind_addr,
            error: None,
        };
    }

    pub async fn mark_server_failed(&self, bind_addr: String, error: String) {
        let mut status = self.inner.server_status.write().await;
        *status = ServerStatus {
            state: ServerState::Failed,
            bind_addr,
            error: Some(error),
        };
    }

    pub async fn local_info(&self) -> LocalInfo {
        LocalInfo {
            device_id: self.inner.config.device_id.clone(),
            device_name: self.inner.config.device_name.clone(),
            version: self.inner.config.version.clone(),
            platform: current_platform().to_string(),
            protocol_version: self.inner.config.protocol_version,
            listen_host: self.inner.config.listen_host.clone(),
            listen_port: self.inner.config.listen_port,
            download_dir: self.inner.config.download_dir.clone(),
            config_path: self.inner.config_path.clone(),
            database_path: self.inner.store.db_path().display().to_string(),
            features: features(),
            server_status: self.inner.server_status.read().await.clone(),
        }
    }

    pub async fn ping_response(&self) -> PingResponse {
        PingResponse {
            device_id: self.inner.config.device_id.clone(),
            device_name: self.inner.config.device_name.clone(),
            version: self.inner.config.version.clone(),
            platform: current_platform().to_string(),
            protocol_version: self.inner.config.protocol_version,
            features: features(),
        }
    }

    pub async fn agent_status_event(&self) -> AgentStatusEvent {
        AgentStatusEvent {
            timestamp: unix_millis(),
            local_info: self.local_info().await,
        }
    }
}

fn features() -> Vec<String> {
    vec![
        "ping".to_string(),
        "http_transport".to_string(),
        "ws_control".to_string(),
        "direct_file_upload".to_string(),
    ]
}

fn unix_millis() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or_default()
}

fn current_platform() -> &'static str {
    if cfg!(target_os = "macos") {
        "macos"
    } else if cfg!(target_os = "windows") {
        "windows"
    } else if cfg!(target_os = "linux") {
        "linux"
    } else {
        "unknown"
    }
}
