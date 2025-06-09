// embedded static asset handlers

use axum::{
    body::Body,
    extract::Path,
    http::{header, StatusCode},
    response::Response,
};
use rust_embed::RustEmbed;
use tracing::{info, warn};

#[derive(RustEmbed)]
#[folder = "assets/"]
#[include = "*.css"]
#[include = "*.svg"]
#[include = "*.ico"]
pub struct StaticAssets;

/// serve embedded static assets
pub async fn serve_static_asset(Path(asset_path): Path<String>) -> Result<Response, StatusCode> {
    info!("serving static asset: {}", asset_path);

    // get embedded asset
    let asset = StaticAssets::get(&asset_path).ok_or_else(|| {
        warn!("static asset not found: {}", asset_path);
        StatusCode::NOT_FOUND
    })?;

    // determine mime type based on file extension
    let mime_type = get_asset_mime_type(&asset_path);

    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, mime_type)
        .header(header::CACHE_CONTROL, "public, max-age=31536000") // 1 year cache
        .header(header::CONTENT_LENGTH, asset.data.len())
        .body(Body::from(asset.data.to_vec()))
        .map_err(|e| {
            warn!("failed to build asset response: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })
}

/// serve embedded favicon.ico specifically
pub async fn serve_embedded_favicon() -> Result<Response, StatusCode> {
    info!("serving embedded favicon.ico");

    let asset = StaticAssets::get("favicon.ico").ok_or_else(|| {
        warn!("embedded favicon.ico not found");
        StatusCode::NOT_FOUND
    })?;

    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "image/x-icon")
        .header(header::CACHE_CONTROL, "public, max-age=31536000")
        .header(header::CONTENT_LENGTH, asset.data.len())
        .body(Body::from(asset.data.to_vec()))
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

/// determine mime type for static assets
fn get_asset_mime_type(file_path: &str) -> &'static str {
    if file_path.ends_with(".css") {
        "text/css"
    } else if file_path.ends_with(".svg") {
        "image/svg+xml"
    } else if file_path.ends_with(".ico") {
        "image/x-icon"
    } else if file_path.ends_with(".png") {
        "image/png"
    } else if file_path.ends_with(".jpg") || file_path.ends_with(".jpeg") {
        "image/jpeg"
    } else {
        "application/octet-stream"
    }
}
