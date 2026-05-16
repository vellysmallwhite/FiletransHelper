use std::collections::HashMap;
use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use serde_json::json;
use tauri::async_runtime::JoinHandle;
use tokio::sync::RwLock;
use tokio_tungstenite::{connect_async, tungstenite::Message};
use uuid::Uuid;

use crate::device::manager::Device;
use crate::server::ws::{
    emit_ws_connection_changed, serialize_event, unix_millis, WsConnectionChangedEvent, WsEvent,
    WsHub,
};

const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(5);
const HEARTBEAT_TIMEOUT: Duration = Duration::from_secs(15);
const RECONNECT_BACKOFFS: [Duration; 3] = [
    Duration::from_secs(1),
    Duration::from_secs(3),
    Duration::from_secs(10),
];

#[derive(Clone, Debug)]
pub struct WsClientManager {
    hub: WsHub,
    tasks: std::sync::Arc<RwLock<HashMap<String, JoinHandle<()>>>>,
}

impl WsClientManager {
    pub fn new(hub: WsHub) -> Self {
        Self {
            hub,
            tasks: std::sync::Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub async fn connect_device(&self, device: Device, state: crate::app_state::AppState) {
        if device.id == self.hub.local().device_id {
            return;
        }

        if self.hub.is_connected(&device.id).await {
            return;
        }

        let peer_device_id = device.id.clone();
        if let Some(existing) = self.tasks.write().await.remove(&peer_device_id) {
            existing.abort();
        }

        let manager = self.clone();
        let handle = tauri::async_runtime::spawn(async move {
            manager.run_connect_loop(device, state).await;
        });

        self.tasks.write().await.insert(peer_device_id, handle);
    }

    pub async fn connect_all(&self, state: crate::app_state::AppState) -> Result<(), String> {
        let devices = state.device_manager().list_devices()?;
        for device in devices {
            self.connect_device(device, state.clone()).await;
        }

        Ok(())
    }

    async fn run_connect_loop(&self, device: Device, state: crate::app_state::AppState) {
        let mut attempt = 0usize;

        loop {
            if self.hub.is_connected(&device.id).await {
                return;
            }

            match self.connect_once(&device, state.clone()).await {
                Ok(()) => {
                    attempt = 0;
                }
                Err(err) => {
                    let app_handle = state.app_handle().await;
                    emit_ws_connection_changed(
                        &app_handle,
                        WsConnectionChangedEvent {
                            peer_device_id: device.id.clone(),
                            peer_device_name: Some(device.name.clone()),
                            connected: false,
                            state: "reconnecting".to_string(),
                            last_event_at: Some(unix_millis()),
                            error: Some(err),
                        },
                    );
                    let delay =
                        RECONNECT_BACKOFFS[attempt.min(RECONNECT_BACKOFFS.len().saturating_sub(1))];
                    attempt = attempt.saturating_add(1);
                    tokio::time::sleep(delay).await;
                }
            }
        }
    }

    async fn connect_once(
        &self,
        device: &Device,
        state: crate::app_state::AppState,
    ) -> Result<(), String> {
        let url = format!(
            "ws://{}:{}/api/ws?deviceId={}",
            device.ip,
            device.port,
            self.hub.local().device_id
        );
        let (ws_stream, _) = connect_async(&url)
            .await
            .map_err(|err| format!("connect {url} failed: {err}"))?;
        let (mut ws_sender, mut ws_receiver) = ws_stream.split();
        let (sender, mut receiver) =
            tokio::sync::mpsc::unbounded_channel::<axum::extract::ws::Message>();
        let app_handle = state.app_handle().await;
        let Some(session_id) = self
            .hub
            .register_peer_sender(
                device.id.clone(),
                Some(device.name.clone()),
                sender,
                app_handle.clone(),
            )
            .await
        else {
            return Ok(());
        };

        let result = async {
            let online_event = WsEvent::new("deviceOnline", self.hub.local(), json!({}));
            self.send_tungstenite_event(&mut ws_sender, &online_event)
                .await?;
            let hello_event =
                WsEvent::new("hello", self.hub.local(), json!({ "protocolVersion": 1 }));
            self.send_tungstenite_event(&mut ws_sender, &hello_event)
                .await?;

            let mut last_seen = unix_millis();
            let mut heartbeat = tokio::time::interval(HEARTBEAT_INTERVAL);

            loop {
                tokio::select! {
                    outbound = receiver.recv() => {
                        let Some(outbound) = outbound else {
                            break Ok(());
                        };

                        match axum_to_tungstenite(outbound) {
                            Some(message) => {
                                ws_sender
                                    .send(message)
                                    .await
                                    .map_err(|err| format!("WebSocket 写入失败: {err}"))?;
                            }
                            None => break Ok(()),
                        }
                    }
                    inbound = ws_receiver.next() => {
                        match inbound {
                            Some(Ok(Message::Text(text))) => {
                                last_seen = unix_millis();
                                self.hub.handle_peer_text(&device.id, &app_handle, &text).await;
                            }
                            Some(Ok(Message::Pong(_))) | Some(Ok(Message::Ping(_))) => {
                                last_seen = unix_millis();
                                self.hub.mark_event(&device.id).await;
                            }
                            Some(Ok(Message::Close(_))) | None => break Ok(()),
                            Some(Ok(Message::Binary(_))) | Some(Ok(Message::Frame(_))) => {}
                            Some(Err(err)) => {
                                break Err(format!("WebSocket 读取失败: {err}"));
                            }
                        }
                    }
                    _ = heartbeat.tick() => {
                        if unix_millis().saturating_sub(last_seen) > duration_millis(HEARTBEAT_TIMEOUT) {
                            break Err("WebSocket 心跳超时".to_string());
                        }

                        let heartbeat_event = WsEvent::new(
                            "heartbeat",
                            self.hub.local(),
                            json!({ "nonce": Uuid::new_v4().simple().to_string() }),
                        );
                        self.send_tungstenite_event(&mut ws_sender, &heartbeat_event)
                            .await?;
                    }
                }
            }
        }
        .await;

        self.hub
            .unregister_peer_session(
                &device.id,
                Some(&session_id),
                app_handle,
                result.as_ref().err().cloned(),
            )
            .await;
        result
    }

    async fn send_tungstenite_event<S>(&self, sender: &mut S, event: &WsEvent) -> Result<(), String>
    where
        S: futures_util::Sink<Message> + Unpin,
        S::Error: std::fmt::Display,
    {
        sender
            .send(Message::Text(serialize_event(event)?))
            .await
            .map_err(|err| format!("WebSocket 写入失败: {err}"))
    }
}

fn axum_to_tungstenite(message: axum::extract::ws::Message) -> Option<Message> {
    match message {
        axum::extract::ws::Message::Text(text) => Some(Message::Text(text)),
        axum::extract::ws::Message::Binary(bytes) => Some(Message::Binary(bytes)),
        axum::extract::ws::Message::Ping(bytes) => Some(Message::Ping(bytes)),
        axum::extract::ws::Message::Pong(bytes) => Some(Message::Pong(bytes)),
        axum::extract::ws::Message::Close(_) => Some(Message::Close(None)),
    }
}

fn duration_millis(duration: Duration) -> i64 {
    i64::try_from(duration.as_millis()).unwrap_or(i64::MAX)
}
