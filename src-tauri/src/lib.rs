mod app_state;
mod config;
mod device;
mod events;
mod message;
mod security;
mod server;
mod store;
mod transfer;
mod transport;

use app_state::AppState;
use device::manager::Device;
use message::manager::ChatMessage;
use server::ws::WsConnectionInfo;
use tauri::AppHandle;
use tauri::State;
use transfer::manager::FileTransfer;

#[tauri::command]
async fn get_local_info(state: State<'_, AppState>) -> Result<app_state::LocalInfo, String> {
    Ok(state.local_info().await)
}

#[tauri::command]
fn hello_from_rust(name: String) -> String {
    format!("Hello, {name}. Rust Core is ready.")
}

#[tauri::command]
fn list_devices(state: State<'_, AppState>) -> Result<Vec<Device>, String> {
    state.device_manager().list_devices()
}

#[tauri::command]
async fn add_device(address: String, state: State<'_, AppState>) -> Result<Device, String> {
    let device = state.device_manager().add_device(&address).await?;
    state.connect_device_ws(&device.id).await?;
    Ok(device)
}

#[tauri::command]
async fn refresh_device_status(state: State<'_, AppState>) -> Result<Vec<Device>, String> {
    state.device_manager().refresh_device_status().await
}

#[tauri::command]
fn get_messages(
    peer_device_id: String,
    state: State<'_, AppState>,
) -> Result<Vec<ChatMessage>, String> {
    state.message_manager().get_messages(&peer_device_id)
}

#[tauri::command]
fn get_transfers(
    peer_device_id: Option<String>,
    state: State<'_, AppState>,
) -> Result<Vec<FileTransfer>, String> {
    state
        .transfer_manager()
        .get_transfers(peer_device_id.as_deref())
}

#[tauri::command]
async fn send_text(
    peer_device_id: String,
    content: String,
    state: State<'_, AppState>,
    app: AppHandle,
) -> Result<ChatMessage, String> {
    state
        .message_manager()
        .send_text(&peer_device_id, &content, &app)
        .await
}

#[tauri::command]
async fn send_file(
    peer_device_id: String,
    file_path: String,
    state: State<'_, AppState>,
    app: AppHandle,
) -> Result<FileTransfer, String> {
    state
        .transfer_manager()
        .send_file(&peer_device_id, &file_path, &app)
        .await
}

#[tauri::command]
async fn retry_message(
    message_id: String,
    state: State<'_, AppState>,
    app: AppHandle,
) -> Result<ChatMessage, String> {
    state
        .message_manager()
        .retry_message(&message_id, &app)
        .await
}

#[tauri::command]
async fn connect_device_ws(
    peer_device_id: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    state.connect_device_ws(&peer_device_id).await
}

#[tauri::command]
async fn connect_all_device_ws(state: State<'_, AppState>) -> Result<(), String> {
    state.connect_all_device_ws().await
}

#[tauri::command]
async fn list_ws_connections(state: State<'_, AppState>) -> Result<Vec<WsConnectionInfo>, String> {
    Ok(state.list_ws_connections().await)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let app_state = AppState::load_or_init().unwrap_or_else(|err| {
        panic!("failed to initialize ZeroDrop state: {err}");
    });

    tauri::Builder::default()
        .manage(app_state.clone())
        .setup(move |app| {
            let app_handle = app.handle().clone();
            tauri::async_runtime::block_on(app_state.set_app_handle(app_handle.clone()));
            events::emitter::start_agent_status_heartbeat(app_handle, app_state.clone());
            server::http::spawn(app_state.clone());
            let ws_state = app_state.clone();
            tauri::async_runtime::spawn(async move {
                tokio::time::sleep(std::time::Duration::from_millis(800)).await;
                if let Err(err) = ws_state.connect_all_device_ws().await {
                    eprintln!("connect all device ws failed: {err}");
                }
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_local_info,
            hello_from_rust,
            list_devices,
            add_device,
            refresh_device_status,
            get_messages,
            get_transfers,
            send_text,
            send_file,
            retry_message,
            connect_device_ws,
            connect_all_device_ws,
            list_ws_connections
        ])
        .run(tauri::generate_context!())
        .expect("error while running ZeroDrop");
}
