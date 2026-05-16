use std::collections::HashMap;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use axum::extract::ws::{Message, WebSocket};
use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tauri::{AppHandle, Emitter};
use tokio::sync::{mpsc, RwLock};
use uuid::Uuid;

use crate::device::manager::Device;

const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(5);
const HEARTBEAT_TIMEOUT: Duration = Duration::from_secs(15);

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WsEvent {
    pub event_id: String,
    pub event_type: String,
    pub from_device_id: String,
    pub from_device_name: String,
    pub created_at: i64,
    pub payload: Value,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WsConnectionInfo {
    pub peer_device_id: String,
    pub peer_device_name: Option<String>,
    pub connected: bool,
    pub state: String,
    pub last_event_at: Option<i64>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceWsEvent {
    pub peer_device_id: String,
    pub peer_device_name: Option<String>,
    pub event: WsEvent,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WsConnectionChangedEvent {
    pub peer_device_id: String,
    pub peer_device_name: Option<String>,
    pub connected: bool,
    pub state: String,
    pub last_event_at: Option<i64>,
    pub error: Option<String>,
}

#[derive(Clone, Debug)]
pub struct LocalWsIdentity {
    pub device_id: String,
    pub device_name: String,
}

#[derive(Clone, Debug)]
pub struct WsHub {
    inner: std::sync::Arc<RwLock<HashMap<String, PeerSession>>>,
    local: LocalWsIdentity,
}

#[derive(Clone, Debug)]
struct PeerSession {
    session_id: String,
    peer_device_id: String,
    peer_device_name: Option<String>,
    sender: mpsc::UnboundedSender<Message>,
    state: String,
    last_event_at: Option<i64>,
}

impl WsEvent {
    pub fn new(event_type: impl Into<String>, local: &LocalWsIdentity, payload: Value) -> Self {
        Self {
            event_id: format!("evt_{}", Uuid::new_v4().simple()),
            event_type: event_type.into(),
            from_device_id: local.device_id.clone(),
            from_device_name: local.device_name.clone(),
            created_at: unix_millis(),
            payload,
        }
    }

    fn hello(local: &LocalWsIdentity) -> Self {
        Self::new("hello", local, json!({ "protocolVersion": 1 }))
    }

    fn heartbeat(local: &LocalWsIdentity) -> Self {
        Self::new("heartbeat", local, json!({}))
    }
}

impl WsHub {
    pub fn new(device_id: String, device_name: String) -> Self {
        Self {
            inner: std::sync::Arc::new(RwLock::new(HashMap::new())),
            local: LocalWsIdentity {
                device_id,
                device_name,
            },
        }
    }

    pub fn local(&self) -> &LocalWsIdentity {
        &self.local
    }

    pub async fn is_connected(&self, peer_device_id: &str) -> bool {
        self.inner.read().await.contains_key(peer_device_id)
    }

    pub async fn list_connections(&self) -> Vec<WsConnectionInfo> {
        self.inner
            .read()
            .await
            .values()
            .map(|session| WsConnectionInfo {
                peer_device_id: session.peer_device_id.clone(),
                peer_device_name: session.peer_device_name.clone(),
                connected: true,
                state: session.state.clone(),
                last_event_at: session.last_event_at,
            })
            .collect()
    }

    pub async fn send_event(&self, peer_device_id: &str, event: &WsEvent) -> Result<(), String> {
        let text = serialize_event(event)?;
        let sender = self
            .inner
            .read()
            .await
            .get(peer_device_id)
            .map(|session| session.sender.clone())
            .ok_or_else(|| "WebSocket 控制通道未连接".to_string())?;

        sender
            .send(Message::Text(text))
            .map_err(|_| "WebSocket 控制通道已断开".to_string())
    }

    pub async fn register_peer_sender(
        &self,
        peer_device_id: String,
        peer_device_name: Option<String>,
        sender: mpsc::UnboundedSender<Message>,
        app_handle: Option<AppHandle>,
    ) -> Option<String> {
        let session_id = format!("wss_{}", Uuid::new_v4().simple());
        let session = PeerSession {
            session_id: session_id.clone(),
            peer_device_id: peer_device_id.clone(),
            peer_device_name: peer_device_name.clone(),
            sender,
            state: "connected".to_string(),
            last_event_at: Some(unix_millis()),
        };

        {
            let mut sessions = self.inner.write().await;
            if sessions.contains_key(&peer_device_id) {
                return None;
            }
            sessions.insert(peer_device_id.clone(), session);
        }

        let event = WsEvent::new("deviceOnline", &self.local, json!({}));
        emit_device_online(
            &app_handle,
            &peer_device_id,
            peer_device_name.clone(),
            &event,
        );
        emit_ws_connection_changed(
            &app_handle,
            WsConnectionChangedEvent {
                peer_device_id,
                peer_device_name,
                connected: true,
                state: "connected".to_string(),
                last_event_at: Some(event.created_at),
                error: None,
            },
        );

        Some(session_id)
    }

    pub async fn mark_event(&self, peer_device_id: &str) {
        if let Some(session) = self.inner.write().await.get_mut(peer_device_id) {
            session.last_event_at = Some(unix_millis());
        }
    }

    pub async fn unregister_peer_session(
        &self,
        peer_device_id: &str,
        session_id: Option<&str>,
        app_handle: Option<AppHandle>,
        error: Option<String>,
    ) {
        let removed = {
            let mut sessions = self.inner.write().await;
            let should_remove = sessions
                .get(peer_device_id)
                .map(|session| {
                    session_id
                        .map(|expected| expected == session.session_id)
                        .unwrap_or(true)
                })
                .unwrap_or(false);

            if should_remove {
                sessions.remove(peer_device_id)
            } else {
                None
            }
        };

        if let Some(session) = removed {
            let event = WsEvent::new(
                "deviceOffline",
                &self.local,
                json!({ "error": error.clone() }),
            );
            emit_device_offline(
                &app_handle,
                &session.peer_device_id,
                session.peer_device_name.clone(),
                &event,
            );
            emit_ws_connection_changed(
                &app_handle,
                WsConnectionChangedEvent {
                    peer_device_id: session.peer_device_id,
                    peer_device_name: session.peer_device_name,
                    connected: false,
                    state: "disconnected".to_string(),
                    last_event_at: Some(event.created_at),
                    error,
                },
            );
        }
    }

    pub async fn handle_peer_text(
        &self,
        peer_device_id: &str,
        app_handle: &Option<AppHandle>,
        text: &str,
    ) {
        handle_peer_text(self, peer_device_id, app_handle, text).await;
    }

    pub async fn handle_inbound_socket(
        &self,
        socket: WebSocket,
        peer: Device,
        app_handle: Option<AppHandle>,
    ) {
        let (mut ws_sender, mut ws_receiver) = socket.split();
        let (sender, mut receiver) = mpsc::unbounded_channel::<Message>();
        let peer_device_id = peer.id.clone();
        let peer_device_name = Some(peer.name.clone());

        let Some(session_id) = self
            .register_peer_sender(
                peer_device_id.clone(),
                peer_device_name,
                sender,
                app_handle.clone(),
            )
            .await
        else {
            return;
        };

        let hello = WsEvent::hello(&self.local);
        if let Err(err) = self.send_event(&peer_device_id, &hello).await {
            self.unregister_peer_session(&peer_device_id, Some(&session_id), app_handle, Some(err))
                .await;
            return;
        }

        let writer = async move {
            while let Some(message) = receiver.recv().await {
                if ws_sender.send(message).await.is_err() {
                    break;
                }
            }
        };

        let hub = self.clone();
        let reader_peer_id = peer_device_id.clone();
        let reader_session_id = session_id.clone();
        let reader_app = app_handle.clone();
        let reader = async move {
            let mut last_seen = unix_millis();
            let mut heartbeat = tokio::time::interval(HEARTBEAT_INTERVAL);
            loop {
                tokio::select! {
                    message = ws_receiver.next() => {
                        match message {
                            Some(Ok(Message::Text(text))) => {
                                last_seen = unix_millis();
                                handle_peer_text(&hub, &reader_peer_id, &reader_app, &text).await;
                            }
                            Some(Ok(Message::Pong(_))) | Some(Ok(Message::Ping(_))) => {
                                last_seen = unix_millis();
                                hub.mark_event(&reader_peer_id).await;
                            }
                            Some(Ok(Message::Close(_))) | None => break,
                            Some(Ok(Message::Binary(_))) => {}
                            Some(Err(err)) => {
                                hub.unregister_peer_session(
                                    &reader_peer_id,
                                    Some(&reader_session_id),
                                    reader_app.clone(),
                                    Some(format!("WebSocket 读取失败: {err}")),
                                )
                                .await;
                                return;
                            }
                        }
                    }
                    _ = heartbeat.tick() => {
                        if unix_millis().saturating_sub(last_seen) > duration_millis(HEARTBEAT_TIMEOUT) {
                            hub.unregister_peer_session(
                                &reader_peer_id,
                                Some(&reader_session_id),
                                reader_app.clone(),
                                Some("WebSocket 心跳超时".to_string()),
                            )
                            .await;
                            return;
                        }

                        let heartbeat_event = WsEvent::heartbeat(hub.local());
                        if let Err(err) = hub.send_event(&reader_peer_id, &heartbeat_event).await {
                            hub.unregister_peer_session(
                                &reader_peer_id,
                                Some(&reader_session_id),
                                reader_app.clone(),
                                Some(err),
                            )
                            .await;
                            return;
                        }
                    }
                }
            }

            hub.unregister_peer_session(
                &reader_peer_id,
                Some(&reader_session_id),
                reader_app,
                None,
            )
            .await;
        };

        tokio::select! {
            _ = writer => {}
            _ = reader => {}
        }

        self.unregister_peer_session(&peer_device_id, Some(&session_id), app_handle, None)
            .await;
    }
}

async fn handle_peer_text(
    hub: &WsHub,
    peer_device_id: &str,
    app_handle: &Option<AppHandle>,
    text: &str,
) {
    match serde_json::from_str::<WsEvent>(text) {
        Ok(event) => {
            hub.mark_event(peer_device_id).await;

            match event.event_type.as_str() {
                "hello" | "deviceOnline" => {
                    emit_device_online(
                        app_handle,
                        peer_device_id,
                        Some(event.from_device_name.clone()),
                        &event,
                    );
                }
                "deviceOffline" => {
                    emit_device_offline(
                        app_handle,
                        peer_device_id,
                        Some(event.from_device_name.clone()),
                        &event,
                    );
                }
                "heartbeat" => {}
                _ => {}
            }
        }
        Err(err) => {
            eprintln!("parse ws event from {peer_device_id} failed: {err}");
        }
    }
}

pub fn emit_device_online(
    app_handle: &Option<AppHandle>,
    peer_device_id: &str,
    peer_device_name: Option<String>,
    event: &WsEvent,
) {
    let Some(app) = app_handle else {
        return;
    };

    if let Err(err) = app.emit(
        "device_online",
        DeviceWsEvent {
            peer_device_id: peer_device_id.to_string(),
            peer_device_name,
            event: event.clone(),
        },
    ) {
        eprintln!("emit device_online failed: {err}");
    }
}

pub fn emit_device_offline(
    app_handle: &Option<AppHandle>,
    peer_device_id: &str,
    peer_device_name: Option<String>,
    event: &WsEvent,
) {
    let Some(app) = app_handle else {
        return;
    };

    if let Err(err) = app.emit(
        "device_offline",
        DeviceWsEvent {
            peer_device_id: peer_device_id.to_string(),
            peer_device_name,
            event: event.clone(),
        },
    ) {
        eprintln!("emit device_offline failed: {err}");
    }
}

pub fn emit_ws_connection_changed(app_handle: &Option<AppHandle>, event: WsConnectionChangedEvent) {
    let Some(app) = app_handle else {
        return;
    };

    if let Err(err) = app.emit("ws_connection_changed", event) {
        eprintln!("emit ws_connection_changed failed: {err}");
    }
}

pub fn serialize_event(event: &WsEvent) -> Result<String, String> {
    serde_json::to_string(event).map_err(|err| format!("serialize ws event failed: {err}"))
}

fn duration_millis(duration: Duration) -> i64 {
    i64::try_from(duration.as_millis()).unwrap_or(i64::MAX)
}

pub fn unix_millis() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| i64::try_from(duration.as_millis()).unwrap_or(i64::MAX))
        .unwrap_or_default()
}
