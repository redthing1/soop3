// file upload handlers

use axum::{
    body::Body,
    extract::{Multipart, OriginalUri, State},
    http::StatusCode,
    response::Response,
};
use tracing::{error, info, instrument, warn};

use crate::server::app::AppState;
use crate::server::uploads;

/// handle file upload requests to root directory
#[instrument(skip(state, multipart))]
pub async fn handle_root_upload_request(
    State(state): State<AppState>,
    OriginalUri(uri): OriginalUri,
    multipart: Multipart,
) -> Result<Response, StatusCode> {
    let upload_path = uri.path().trim_start_matches('/');
    handle_upload_impl(state, upload_path, multipart).await
}

/// handle file upload requests with path
#[instrument(skip(state, multipart, uri))]
pub async fn handle_upload_request(
    State(state): State<AppState>,
    OriginalUri(uri): OriginalUri,
    multipart: Multipart,
) -> Result<Response, StatusCode> {
    let upload_path = uri.path().trim_start_matches('/');
    handle_upload_impl(state, upload_path, multipart).await
}

/// internal implementation for upload handling
async fn handle_upload_impl(
    state: AppState,
    upload_path: &str,
    mut multipart: Multipart,
) -> Result<Response, StatusCode> {
    info!("processing upload request");

    // verify uploads are enabled
    if !state.config.server.enable_upload {
        warn!("upload attempt but uploads are disabled");
        return Err(StatusCode::FORBIDDEN);
    }

    while let Some(field) = multipart.next_field().await.map_err(|e| {
        error!("failed to read multipart field: {}", e);
        e.status()
    })? {
        let name = field.name().unwrap_or("").to_string();
        let filename = field.file_name().map(|s| s.to_string());

        info!(
            "processing multipart field: {} (filename: {:?})",
            name, filename
        );

        let filename = match filename {
            Some(filename) => filename,
            None => continue,
        };

        // validate and process upload
        let target_path = uploads::process_upload(&state.config, upload_path, filename, field)
            .await
            .map_err(|err| {
                error!("upload failed: {}", err);
                err.status_code()
            })?;

        info!("upload completed successfully: {}", target_path.display());

        // return success with no content
        return Response::builder()
            .status(StatusCode::NO_CONTENT)
            .body(Body::empty())
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR);
    }

    warn!("no file found in upload request");
    Err(StatusCode::BAD_REQUEST)
}
