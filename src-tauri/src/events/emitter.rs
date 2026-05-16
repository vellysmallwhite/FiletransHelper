use std::time::Duration;

use tauri::{AppHandle, Emitter};

use crate::app_state::AppState;

pub fn start_agent_status_heartbeat(app: AppHandle, state: AppState) {
    tauri::async_runtime::spawn(async move {
        loop {
            let payload = state.agent_status_event().await;
            if let Err(err) = app.emit("agent_status", payload) {
                eprintln!("emit agent_status failed: {err}");
            }

            tokio::time::sleep(Duration::from_secs(3)).await;
        }
    });
}
