use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use axum::body::Bytes;
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter};
use uuid::Uuid;

use crate::device::manager::{Device, DeviceEndpoint};
use crate::store::sqlite::SqliteStore;
use crate::transport::http::HttpTransport;

pub const MAX_DIRECT_FILE_BYTES: u64 = 100 * 1024 * 1024;

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileTransfer {
    pub id: String,
    pub peer_device_id: String,
    pub peer_device_name: Option<String>,
    pub direction: String,
    pub filename: String,
    pub size: i64,
    pub status: String,
    pub local_path: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
    pub error: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileUploadQuery {
    pub transfer_id: String,
    pub from_device_id: String,
    pub from_device_name: String,
    pub to_device_id: String,
    pub filename: String,
    pub size: i64,
    pub created_at: i64,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileUploadResponse {
    pub ok: bool,
    pub transfer: FileTransfer,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TransferCreatedEvent {
    pub transfer: FileTransfer,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TransferStatusChangedEvent {
    pub transfer: FileTransfer,
}

#[derive(Clone, Debug)]
pub struct TransferManager {
    store: SqliteStore,
    transport: HttpTransport,
    local_device_id: String,
    local_device_name: String,
    download_dir: String,
}

impl TransferManager {
    pub fn new(
        store: SqliteStore,
        transport: HttpTransport,
        local_device_id: String,
        local_device_name: String,
        download_dir: String,
    ) -> Self {
        Self {
            store,
            transport,
            local_device_id,
            local_device_name,
            download_dir,
        }
    }

    pub fn get_transfers(&self, peer_device_id: Option<&str>) -> Result<Vec<FileTransfer>, String> {
        self.store.list_transfers(peer_device_id)
    }

    pub async fn send_file(
        &self,
        peer_device_id: &str,
        file_path: &str,
        app: &AppHandle,
    ) -> Result<FileTransfer, String> {
        let path = PathBuf::from(file_path);
        let metadata =
            std::fs::metadata(&path).map_err(|err| format!("读取文件信息失败：{err}"))?;
        if !metadata.is_file() {
            return Err("Phase 5 仅支持拖拽单个普通文件".to_string());
        }

        let size = metadata.len();
        if size == 0 {
            return Err("不能发送空文件".to_string());
        }
        if size > MAX_DIRECT_FILE_BYTES {
            return Err("文件超过 100MB，请等待 Phase 6 分片传输".to_string());
        }

        let filename = safe_file_name(&path).ok_or_else(|| "无法识别文件名".to_string())?;
        let peer = self
            .store
            .get_device(peer_device_id)?
            .ok_or_else(|| "发送失败：设备不存在，请先添加设备".to_string())?;
        let now = unix_millis();
        let transfer = FileTransfer {
            id: format!("tr_{}", Uuid::new_v4().simple()),
            peer_device_id: peer.id.clone(),
            peer_device_name: Some(peer.name.clone()),
            direction: "outbound".to_string(),
            filename,
            size: i64::try_from(size).unwrap_or(i64::MAX),
            status: "sending".to_string(),
            local_path: Some(path.display().to_string()),
            created_at: now,
            updated_at: now,
            error: None,
        };

        self.store.save_transfer(&transfer)?;
        emit_transfer_created(app, &transfer);
        self.send_existing(transfer, &peer, &path, app).await
    }

    pub fn receive_direct_upload(
        &self,
        query: FileUploadQuery,
        bytes: Bytes,
    ) -> Result<FileTransfer, String> {
        self.validate_upload_query(&query, bytes.len())?;
        let peer = self
            .store
            .get_device(&query.from_device_id)?
            .ok_or_else(|| "接收失败：请先互相添加设备".to_string())?;
        let filename = sanitize_inbound_filename(&query.filename)?;
        let download_dir = PathBuf::from(&self.download_dir);
        let save_path = unique_download_path(&download_dir, &filename)?;

        let now = unix_millis();
        let mut transfer = FileTransfer {
            id: query.transfer_id,
            peer_device_id: query.from_device_id,
            peer_device_name: Some(if query.from_device_name.trim().is_empty() {
                peer.name
            } else {
                query.from_device_name
            }),
            direction: "inbound".to_string(),
            filename,
            size: query.size,
            status: "receiving".to_string(),
            local_path: Some(save_path.display().to_string()),
            created_at: query.created_at,
            updated_at: now,
            error: None,
        };

        self.store.save_transfer(&transfer)?;

        if let Some(parent) = save_path.parent() {
            if let Err(err) = std::fs::create_dir_all(parent) {
                let message = format!("创建下载目录失败：{err}");
                let _ = self.store.update_transfer_status(
                    &transfer.id,
                    "failed",
                    Some(message.clone()),
                    unix_millis(),
                );
                return Err(message);
            }
        }
        if let Err(err) = std::fs::write(&save_path, &bytes) {
            let message = format!("保存文件失败：{err}");
            let _ = self.store.update_transfer_status(
                &transfer.id,
                "failed",
                Some(message.clone()),
                unix_millis(),
            );
            return Err(message);
        }

        transfer.status = "received".to_string();
        transfer.updated_at = unix_millis();
        self.store.save_transfer(&transfer)?;
        Ok(transfer)
    }

    fn validate_upload_query(
        &self,
        query: &FileUploadQuery,
        actual_size: usize,
    ) -> Result<(), String> {
        if query.transfer_id.trim().is_empty() {
            return Err("transferId 不能为空".to_string());
        }

        if query.from_device_id.trim().is_empty() {
            return Err("fromDeviceId 不能为空".to_string());
        }

        if query.to_device_id != self.local_device_id {
            return Err("文件目标设备不是本机".to_string());
        }

        if query.size <= 0 {
            return Err("文件内容不能为空".to_string());
        }

        if query.size > i64::try_from(MAX_DIRECT_FILE_BYTES).unwrap_or(i64::MAX) {
            return Err("文件超过 100MB，请等待 Phase 6 分片传输".to_string());
        }

        if i64::try_from(actual_size).unwrap_or(i64::MAX) != query.size {
            return Err("文件大小与声明不一致".to_string());
        }

        sanitize_inbound_filename(&query.filename)?;
        Ok(())
    }

    async fn send_existing(
        &self,
        transfer: FileTransfer,
        peer: &Device,
        path: &Path,
        app: &AppHandle,
    ) -> Result<FileTransfer, String> {
        match self.send_existing_inner(&transfer, peer, path).await {
            Ok(sent) => {
                emit_transfer_status(app, &sent);
                Ok(sent)
            }
            Err(err) => {
                let failed = self.store.update_transfer_status(
                    &transfer.id,
                    "failed",
                    Some(err),
                    unix_millis(),
                )?;
                emit_transfer_status(app, &failed);
                Ok(failed)
            }
        }
    }

    async fn send_existing_inner(
        &self,
        transfer: &FileTransfer,
        peer: &Device,
        path: &Path,
    ) -> Result<FileTransfer, String> {
        let bytes = std::fs::read(path).map_err(|err| format!("读取文件失败：{err}"))?;
        if i64::try_from(bytes.len()).unwrap_or(i64::MAX) != transfer.size {
            return Err("发送前文件大小发生变化".to_string());
        }

        let endpoint = DeviceEndpoint {
            host: peer.ip.clone(),
            port: peer.port,
        };
        let query = FileUploadQuery {
            transfer_id: transfer.id.clone(),
            from_device_id: self.local_device_id.clone(),
            from_device_name: self.local_device_name.clone(),
            to_device_id: peer.id.clone(),
            filename: transfer.filename.clone(),
            size: transfer.size,
            created_at: transfer.created_at,
        };

        let _: FileUploadResponse = self
            .transport
            .post_file_upload(&endpoint, "/api/file/upload", &query, bytes)
            .await?;

        self.store
            .update_transfer_status(&transfer.id, "sent", None, unix_millis())
    }
}

pub fn emit_transfer_created(app: &AppHandle, transfer: &FileTransfer) {
    if let Err(err) = app.emit(
        "transfer_created",
        TransferCreatedEvent {
            transfer: transfer.clone(),
        },
    ) {
        eprintln!("emit transfer_created failed: {err}");
    }
}

pub fn emit_transfer_status(app: &AppHandle, transfer: &FileTransfer) {
    if let Err(err) = app.emit(
        "transfer_status_changed",
        TransferStatusChangedEvent {
            transfer: transfer.clone(),
        },
    ) {
        eprintln!("emit transfer_status_changed failed: {err}");
    }
}

fn safe_file_name(path: &Path) -> Option<String> {
    path.file_name()
        .and_then(|name| name.to_str())
        .map(str::trim)
        .filter(|name| !name.is_empty() && *name != "." && *name != "..")
        .map(str::to_string)
}

fn sanitize_inbound_filename(filename: &str) -> Result<String, String> {
    let path = Path::new(filename);
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .map(str::trim)
        .ok_or_else(|| "文件名无效".to_string())?;

    if name.is_empty() || name == "." || name == ".." {
        return Err("文件名无效".to_string());
    }

    if name.contains('/') || name.contains('\\') {
        return Err("文件名不能包含路径".to_string());
    }

    Ok(name.to_string())
}

fn unique_download_path(download_dir: &Path, filename: &str) -> Result<PathBuf, String> {
    let base = sanitize_inbound_filename(filename)?;
    let candidate = download_dir.join(&base);
    if !candidate.exists() {
        return Ok(candidate);
    }

    let stem = Path::new(&base)
        .file_stem()
        .and_then(|name| name.to_str())
        .filter(|name| !name.is_empty())
        .unwrap_or("file");
    let extension = Path::new(&base)
        .extension()
        .and_then(|name| name.to_str())
        .filter(|name| !name.is_empty());

    for index in 1..10_000 {
        let next_name = match extension {
            Some(extension) => format!("{stem} ({index}).{extension}"),
            None => format!("{stem} ({index})"),
        };
        let next = download_dir.join(next_name);
        if !next.exists() {
            return Ok(next);
        }
    }

    Err("无法为接收文件生成不重名路径".to_string())
}

fn unix_millis() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| i64::try_from(duration.as_millis()).unwrap_or(i64::MAX))
        .unwrap_or_default()
}
