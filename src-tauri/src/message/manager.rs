use std::time::{SystemTime, UNIX_EPOCH};

use axum::body::Bytes;
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter};
use uuid::Uuid;

use crate::device::manager::{Device, DeviceEndpoint};
use crate::store::sqlite::SqliteStore;
use crate::transport::http::HttpTransport;

pub const MAX_TEXT_BYTES: usize = 10 * 1024 * 1024;
pub const TEXT_CHUNK_BYTES: usize = 256 * 1024;

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatMessage {
    pub id: String,
    pub peer_device_id: String,
    pub peer_device_name: Option<String>,
    pub direction: String,
    pub content: String,
    pub status: String,
    pub content_size: i64,
    pub chunk_size: i64,
    pub total_chunks: i64,
    pub chunks_done: i64,
    pub created_at: i64,
    pub updated_at: i64,
    pub error: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MessageInitRequest {
    pub message_id: String,
    pub from_device_id: String,
    pub from_device_name: String,
    pub to_device_id: String,
    pub content_size: i64,
    pub chunk_size: i64,
    pub total_chunks: i64,
    pub created_at: i64,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MessageInitResponse {
    pub accepted: bool,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MessageChunkResponse {
    pub ok: bool,
    pub message_id: String,
    pub chunk_index: i64,
    pub chunks_done: i64,
    pub total_chunks: i64,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MessageCompleteRequest {
    pub from_device_id: String,
    pub to_device_id: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MessageCompleteResponse {
    pub ok: bool,
    pub message: ChatMessage,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MessageReceivedEvent {
    pub message: ChatMessage,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MessageStatusChangedEvent {
    pub message: ChatMessage,
}

#[derive(Clone, Debug)]
pub struct MessageManager {
    store: SqliteStore,
    transport: HttpTransport,
    local_device_id: String,
    local_device_name: String,
}

impl MessageManager {
    pub fn new(
        store: SqliteStore,
        transport: HttpTransport,
        local_device_id: String,
        local_device_name: String,
    ) -> Self {
        Self {
            store,
            transport,
            local_device_id,
            local_device_name,
        }
    }

    pub fn get_messages(&self, peer_device_id: &str) -> Result<Vec<ChatMessage>, String> {
        self.store.list_messages(peer_device_id)
    }

    pub async fn send_text(
        &self,
        peer_device_id: &str,
        content: &str,
        app: &AppHandle,
    ) -> Result<ChatMessage, String> {
        validate_content(content)?;
        let peer = self
            .store
            .get_device(peer_device_id)?
            .ok_or_else(|| "发送失败：设备不存在，请先添加设备".to_string())?;
        let now = unix_millis();
        let content_size = byte_len(content);
        let total_chunks = total_chunks(content_size);
        let message = ChatMessage {
            id: format!("msg_{}", Uuid::new_v4().simple()),
            peer_device_id: peer.id.clone(),
            peer_device_name: Some(peer.name.clone()),
            direction: "outbound".to_string(),
            content: content.to_string(),
            status: "sending".to_string(),
            content_size: i64::try_from(content_size).unwrap_or(i64::MAX),
            chunk_size: i64::try_from(TEXT_CHUNK_BYTES).unwrap_or(i64::MAX),
            total_chunks: i64::try_from(total_chunks).unwrap_or(i64::MAX),
            chunks_done: 0,
            created_at: now,
            updated_at: now,
            error: None,
        };

        self.store.save_message(&message)?;
        emit_message_status(app, &message);
        self.send_existing(message, &peer, app).await
    }

    pub async fn retry_message(
        &self,
        message_id: &str,
        app: &AppHandle,
    ) -> Result<ChatMessage, String> {
        let message = self
            .store
            .get_message(message_id)?
            .ok_or_else(|| "消息不存在".to_string())?;

        if message.direction != "outbound" {
            return Err("只能重试发送失败的发出消息".to_string());
        }

        validate_content(&message.content)?;
        let peer = self
            .store
            .get_device(&message.peer_device_id)?
            .ok_or_else(|| "重试失败：设备不存在，请先添加设备".to_string())?;
        let sending =
            self.store
                .update_message_status(&message.id, "sending", 0, None, unix_millis())?;
        emit_message_status(app, &sending);

        self.send_existing(sending, &peer, app).await
    }

    pub fn init_inbound_message(&self, request: MessageInitRequest) -> Result<(), String> {
        self.validate_inbound_request(&request)?;
        let peer = self
            .store
            .get_device(&request.from_device_id)?
            .ok_or_else(|| "接收失败：请先互相添加设备".to_string())?;
        let now = unix_millis();
        let message = ChatMessage {
            id: request.message_id,
            peer_device_id: request.from_device_id,
            peer_device_name: Some(peer.name),
            direction: "inbound".to_string(),
            content: String::new(),
            status: "receiving".to_string(),
            content_size: request.content_size,
            chunk_size: request.chunk_size,
            total_chunks: request.total_chunks,
            chunks_done: 0,
            created_at: request.created_at,
            updated_at: now,
            error: None,
        };

        self.store.prepare_inbound_message(&message)
    }

    pub fn receive_message_chunk(
        &self,
        message_id: &str,
        chunk_index: i64,
        bytes: Bytes,
    ) -> Result<MessageChunkResponse, String> {
        let bytes = bytes.to_vec();
        if bytes.len() > TEXT_CHUNK_BYTES {
            return Err("消息分片超过 256KB".to_string());
        }

        let message =
            self.store
                .save_message_chunk(message_id, chunk_index, &bytes, unix_millis())?;

        Ok(MessageChunkResponse {
            ok: true,
            message_id: message.id,
            chunk_index,
            chunks_done: message.chunks_done,
            total_chunks: message.total_chunks,
        })
    }

    pub fn complete_inbound_message(
        &self,
        message_id: &str,
        request: MessageCompleteRequest,
    ) -> Result<ChatMessage, String> {
        if request.to_device_id != self.local_device_id {
            return Err("消息目标设备不是本机".to_string());
        }

        let message = self
            .store
            .get_message(message_id)?
            .ok_or_else(|| "消息不存在".to_string())?;
        if message.peer_device_id != request.from_device_id {
            return Err("消息发送方与初始化记录不一致".to_string());
        }

        self.store
            .complete_inbound_message(message_id, unix_millis())
    }

    fn validate_inbound_request(&self, request: &MessageInitRequest) -> Result<(), String> {
        if request.to_device_id != self.local_device_id {
            return Err("消息目标设备不是本机".to_string());
        }

        if request.message_id.trim().is_empty() {
            return Err("messageId 不能为空".to_string());
        }

        if request.from_device_id.trim().is_empty() {
            return Err("fromDeviceId 不能为空".to_string());
        }

        if request.content_size <= 0 {
            return Err("消息内容不能为空".to_string());
        }

        if request.content_size > i64::try_from(MAX_TEXT_BYTES).unwrap_or(i64::MAX) {
            return Err("消息超过 10MB 上限".to_string());
        }

        if request.chunk_size != i64::try_from(TEXT_CHUNK_BYTES).unwrap_or(i64::MAX) {
            return Err("消息分片大小不匹配".to_string());
        }

        if request.total_chunks
            != i64::try_from(total_chunks(request.content_size as usize)).unwrap_or(i64::MAX)
        {
            return Err("消息分片数量不正确".to_string());
        }

        Ok(())
    }

    async fn send_existing(
        &self,
        message: ChatMessage,
        peer: &Device,
        app: &AppHandle,
    ) -> Result<ChatMessage, String> {
        match self.send_existing_inner(&message, peer, app).await {
            Ok(sent) => Ok(sent),
            Err(err) => {
                let failed = self.store.update_message_status(
                    &message.id,
                    "failed",
                    message.chunks_done,
                    Some(err),
                    unix_millis(),
                )?;
                emit_message_status(app, &failed);
                Ok(failed)
            }
        }
    }

    async fn send_existing_inner(
        &self,
        message: &ChatMessage,
        peer: &Device,
        app: &AppHandle,
    ) -> Result<ChatMessage, String> {
        let endpoint = DeviceEndpoint {
            host: peer.ip.clone(),
            port: peer.port,
        };
        let init_request = MessageInitRequest {
            message_id: message.id.clone(),
            from_device_id: self.local_device_id.clone(),
            from_device_name: self.local_device_name.clone(),
            to_device_id: peer.id.clone(),
            content_size: message.content_size,
            chunk_size: message.chunk_size,
            total_chunks: message.total_chunks,
            created_at: message.created_at,
        };

        let _: MessageInitResponse = self
            .transport
            .post_json(&endpoint, "/api/messages/init", &init_request)
            .await?;

        for (index, chunk) in message
            .content
            .as_bytes()
            .chunks(TEXT_CHUNK_BYTES)
            .enumerate()
        {
            let path = format!("/api/messages/{}/chunks/{}", message.id, index);
            let _: MessageChunkResponse = self
                .transport
                .put_bytes(&endpoint, &path, chunk.to_vec())
                .await?;
            let updated = self.store.update_message_status(
                &message.id,
                "sending",
                i64::try_from(index + 1).unwrap_or(i64::MAX),
                None,
                unix_millis(),
            )?;
            emit_message_status(app, &updated);
        }

        let complete_request = MessageCompleteRequest {
            from_device_id: self.local_device_id.clone(),
            to_device_id: peer.id.clone(),
        };
        let _: MessageCompleteResponse = self
            .transport
            .post_json(
                &endpoint,
                &format!("/api/messages/{}/complete", message.id),
                &complete_request,
            )
            .await?;

        let sent = self.store.update_message_status(
            &message.id,
            "sent",
            message.total_chunks,
            None,
            unix_millis(),
        )?;
        emit_message_status(app, &sent);
        Ok(sent)
    }
}

pub fn emit_message_received(app: &AppHandle, message: &ChatMessage) {
    if let Err(err) = app.emit(
        "message_received",
        MessageReceivedEvent {
            message: message.clone(),
        },
    ) {
        eprintln!("emit message_received failed: {err}");
    }
}

fn emit_message_status(app: &AppHandle, message: &ChatMessage) {
    if let Err(err) = app.emit(
        "message_status_changed",
        MessageStatusChangedEvent {
            message: message.clone(),
        },
    ) {
        eprintln!("emit message_status_changed failed: {err}");
    }
}

fn validate_content(content: &str) -> Result<(), String> {
    let bytes = content.as_bytes().len();
    if bytes == 0 {
        return Err("消息内容不能为空".to_string());
    }

    if bytes > MAX_TEXT_BYTES {
        return Err("消息超过 10MB 上限".to_string());
    }

    Ok(())
}

fn byte_len(content: &str) -> usize {
    content.as_bytes().len()
}

fn total_chunks(content_size: usize) -> usize {
    content_size.div_ceil(TEXT_CHUNK_BYTES)
}

fn unix_millis() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| i64::try_from(duration.as_millis()).unwrap_or(i64::MAX))
        .unwrap_or_default()
}
