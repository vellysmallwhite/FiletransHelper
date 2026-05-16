use axum::{
    body::Bytes,
    extract::{ws::WebSocketUpgrade, DefaultBodyLimit, Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{get, post, put},
    Json, Router,
};
use serde::Deserialize;

use crate::app_state::{AppState, PingResponse};
use crate::message::manager::{
    emit_message_received, MessageChunkResponse, MessageCompleteRequest, MessageCompleteResponse,
    MessageInitRequest, MessageInitResponse,
};
use crate::transfer::manager::{
    emit_transfer_created, emit_transfer_status, FileUploadQuery, FileUploadResponse,
    MAX_DIRECT_FILE_BYTES,
};

pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/api/ping", get(ping))
        .route("/api/ws", get(ws_upgrade))
        .route("/api/messages/init", post(init_message))
        .route(
            "/api/file/upload",
            post(upload_file).layer(DefaultBodyLimit::max(
                usize::try_from(MAX_DIRECT_FILE_BYTES).unwrap_or(usize::MAX),
            )),
        )
        .route(
            "/api/messages/:message_id/chunks/:chunk_index",
            put(put_message_chunk),
        )
        .route(
            "/api/messages/:message_id/complete",
            post(complete_message),
        )
        .with_state(state)
}

async fn ping(State(state): State<AppState>) -> Json<PingResponse> {
    Json(state.ping_response().await)
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct WsQuery {
    device_id: String,
}

async fn ws_upgrade(
    ws: WebSocketUpgrade,
    State(state): State<AppState>,
    Query(query): Query<WsQuery>,
) -> Result<Response, ApiError> {
    let peer = state
        .device_manager()
        .list_devices()?
        .into_iter()
        .find(|device| device.id == query.device_id && device.trusted)
        .ok_or_else(|| ApiError::forbidden("WebSocket peer is not trusted or not saved"))?;

    let app_handle = state.app_handle().await;
    let hub = state.ws_hub().clone();
    Ok(ws.on_upgrade(move |socket| async move {
        hub.handle_inbound_socket(socket, peer, app_handle).await;
    }))
}

async fn init_message(
    State(state): State<AppState>,
    Json(request): Json<MessageInitRequest>,
) -> Result<Json<MessageInitResponse>, ApiError> {
    state
        .message_manager()
        .init_inbound_message(request)
        .map_err(ApiError::bad_request)?;
    Ok(Json(MessageInitResponse { accepted: true }))
}

async fn put_message_chunk(
    State(state): State<AppState>,
    Path((message_id, chunk_index)): Path<(String, i64)>,
    bytes: Bytes,
) -> Result<Json<MessageChunkResponse>, ApiError> {
    let response = state
        .message_manager()
        .receive_message_chunk(&message_id, chunk_index, bytes)
        .map_err(ApiError::bad_request)?;
    Ok(Json(response))
}

async fn complete_message(
    State(state): State<AppState>,
    Path(message_id): Path<String>,
    Json(request): Json<MessageCompleteRequest>,
) -> Result<Json<MessageCompleteResponse>, ApiError> {
    let message = state
        .message_manager()
        .complete_inbound_message(&message_id, request)
        .map_err(ApiError::bad_request)?;
    if let Some(app_handle) = state.app_handle().await {
        emit_message_received(&app_handle, &message);
    }
    Ok(Json(MessageCompleteResponse { ok: true, message }))
}

async fn upload_file(
    State(state): State<AppState>,
    Query(query): Query<FileUploadQuery>,
    bytes: Bytes,
) -> Result<Json<FileUploadResponse>, ApiError> {
    let transfer = state
        .transfer_manager()
        .receive_direct_upload(query, bytes)
        .map_err(ApiError::bad_request)?;
    if let Some(app_handle) = state.app_handle().await {
        emit_transfer_created(&app_handle, &transfer);
        emit_transfer_status(&app_handle, &transfer);
    }
    Ok(Json(FileUploadResponse { ok: true, transfer }))
}

struct ApiError {
    status: StatusCode,
    message: String,
}

impl ApiError {
    fn bad_request(message: String) -> Self {
        Self {
            status: StatusCode::BAD_REQUEST,
            message,
        }
    }

    fn forbidden(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::FORBIDDEN,
            message: message.into(),
        }
    }
}

impl From<String> for ApiError {
    fn from(message: String) -> Self {
        Self::bad_request(message)
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (self.status, self.message).into_response()
    }
}
