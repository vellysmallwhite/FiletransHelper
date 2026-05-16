use tokio::net::TcpListener;

use crate::app_state::AppState;
use crate::server::routes;

pub fn spawn(state: AppState) {
    tauri::async_runtime::spawn(async move {
        state.mark_server_starting().await;
        let bind_addr = state.bind_addr();

        if let Err(err) = run(state.clone(), bind_addr.clone()).await {
            state.mark_server_failed(bind_addr, err.to_string()).await;
            eprintln!("ZeroDrop HTTP server failed: {err}");
        }
    });
}

async fn run(
    state: AppState,
    bind_addr: String,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let listener = TcpListener::bind(&bind_addr).await?;
    let actual_addr = listener.local_addr()?.to_string();
    state.mark_server_running(actual_addr).await;

    axum::serve(listener, routes::router(state)).await?;
    Ok(())
}
