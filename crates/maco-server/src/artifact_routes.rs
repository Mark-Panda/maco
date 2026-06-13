//! 会话附件上传、下载与预览路由。

use axum::{
    Json, Router,
    extract::{Multipart, Path, State},
    http::{StatusCode, header},
    response::IntoResponse,
    routing::get,
};
use maco_core::MacoError;

use crate::AppState;
use crate::routes::ApiError;

/// 附件路由，挂载于 `/api` 下。
pub fn router() -> Router<AppState> {
    Router::new()
        .route(
            "/sessions/{id}/artifacts",
            get(list_artifacts).post(upload_artifact),
        )
        .route(
            "/sessions/{id}/artifacts/{artifact_id}",
            get(download_artifact),
        )
        .route(
            "/sessions/{id}/artifacts/{artifact_id}/preview",
            get(preview_artifact),
        )
}

/// `GET /sessions/{id}/artifacts/{artifact_id}/preview` — 返回可预览的文本内容或元数据。
async fn preview_artifact(
    State(state): State<AppState>,
    Path((session_id, artifact_id)): Path<(String, String)>,
) -> Result<Json<serde_json::Value>, ApiError> {
    use maco_governance::is_previewable_mime;

    const PREVIEW_TEXT_LIMIT: usize = 512 * 1024;
    let (record, bytes) = state
        .storage
        .artifacts
        .read(&session_id, &artifact_id)
        .await?;
    let previewable = is_previewable_mime(&record.mime_type);
    let kind = if record.mime_type.starts_with("image/") {
        "image"
    } else if record.mime_type.starts_with("text/")
        || record.mime_type == "application/json"
        || record.mime_type == "application/javascript"
        || record.mime_type == "application/xml"
    {
        "text"
    } else {
        "binary"
    };
    let mut content: Option<String> = None;
    let mut truncated = false;
    if previewable && kind == "text" {
        let text = String::from_utf8_lossy(&bytes);
        let limit = PREVIEW_TEXT_LIMIT;
        if text.len() > limit {
            content = Some(text[..limit].to_string());
            truncated = true;
        } else {
            content = Some(text.into_owned());
        }
    }
    Ok(Json(serde_json::json!({
        "id": record.id,
        "filename": record.filename,
        "mime_type": record.mime_type,
        "size_bytes": record.size_bytes,
        "previewable": previewable,
        "kind": kind,
        "content": content,
        "truncated": truncated,
    })))
}

/// `GET /sessions/{id}/artifacts/{artifact_id}` — 下载附件二进制。
async fn download_artifact(
    State(state): State<AppState>,
    Path((session_id, artifact_id)): Path<(String, String)>,
) -> Result<impl IntoResponse, ApiError> {
    let (record, bytes) = state
        .storage
        .artifacts
        .read(&session_id, &artifact_id)
        .await?;
    let disposition = format!(
        "attachment; filename=\"{}\"",
        record.filename.replace('"', "_")
    );
    Ok((
        StatusCode::OK,
        [
            (header::CONTENT_TYPE, record.mime_type),
            (header::CONTENT_DISPOSITION, disposition),
        ],
        bytes,
    ))
}

/// `GET /sessions/{id}/artifacts` — 列出会话已上传附件元数据。
async fn list_artifacts(
    State(state): State<AppState>,
    Path(session_id): Path<String>,
) -> Result<Json<Vec<maco_db::ArtifactRecord>>, ApiError> {
    Ok(Json(
        state
            .storage
            .artifacts
            .repo()
            .list_for_session(&session_id)
            .await?,
    ))
}

/// `POST /sessions/{id}/artifacts` — multipart 上传附件（字段名 `file`）。
async fn upload_artifact(
    State(state): State<AppState>,
    Path(session_id): Path<String>,
    mut multipart: Multipart,
) -> Result<Json<maco_db::ArtifactRecord>, ApiError> {
    let mut filename = "upload.bin".to_string();
    let mut mime = "application/octet-stream".to_string();
    let mut bytes: Vec<u8> = Vec::new();

    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|e| MacoError::config(e.to_string()))?
    {
        let name = field.name().unwrap_or_default().to_string();
        if name == "file" {
            filename = field.file_name().unwrap_or("upload.bin").to_string();
            mime = field
                .content_type()
                .map(str::to_string)
                .unwrap_or_else(|| "application/octet-stream".into());
            bytes = field
                .bytes()
                .await
                .map_err(|e| MacoError::config(e.to_string()))?
                .to_vec();
        }
    }

    Ok(Json(
        state
            .storage
            .artifacts
            .save(&session_id, &filename, &mime, &bytes)
            .await?,
    ))
}
