// file serving request handlers

use axum::{
    body::Body,
    extract::{OriginalUri, State},
    http::{HeaderMap, StatusCode, header},
    response::Response,
};
use http_range_header::parse_range_header as parse_http_range;
use std::io::ErrorKind;
use std::path::{Path as StdPath, PathBuf};
use tokio::fs::{self as tokio_fs, File};
use tokio::io::{AsyncReadExt, AsyncSeekExt};
use tokio_util::io::ReaderStream;
use tracing::{debug, error, info, instrument, warn};

use super::assets::serve_embedded_favicon;
use crate::server::{app::AppState, fs, listing};

// handle root directory request
#[instrument(skip(state, headers, uri))]
pub async fn handle_root_request(
    State(state): State<AppState>,
    OriginalUri(uri): OriginalUri,
    headers: HeaderMap,
) -> Result<Response, StatusCode> {
    handle_request_internal(state, uri.path().to_string(), headers).await
}

// main request handler - routes to file or directory handling
#[instrument(skip(state, headers, uri))]
pub async fn handle_request(
    State(state): State<AppState>,
    OriginalUri(uri): OriginalUri,
    headers: HeaderMap,
) -> Result<Response, StatusCode> {
    handle_request_internal(state, uri.path().to_string(), headers).await
}

// internal request handling logic
async fn handle_request_internal(
    state: AppState,
    file_path: String,
    headers: HeaderMap,
) -> Result<Response, StatusCode> {
    info!("processing GET request");

    // validate and resolve path securely
    let resolved_path = match fs::resolve_request_path(&state.config.server.public_dir, &file_path)
    {
        Ok(path) => path,
        Err(e) => {
            warn!("rejecting request with bad path: {} - {}", file_path, e);
            return Err(StatusCode::BAD_REQUEST);
        }
    };

    debug!("resolved path: {}", resolved_path.display());

    let metadata = match tokio_fs::metadata(&resolved_path).await {
        Ok(metadata) => Some(metadata),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => None,
        Err(err) => {
            error!(
                "failed to read metadata for {}: {}",
                resolved_path.display(),
                err
            );
            return Err(map_io_error(&err));
        }
    };

    // special case: favicon.ico
    if file_path.ends_with("/favicon.ico") && metadata.is_none() {
        info!("serving embedded favicon.ico");
        return serve_embedded_favicon().await;
    }

    // check if path exists
    let Some(metadata) = metadata else {
        error!("path does not exist: {}", resolved_path.display());
        return Err(StatusCode::NOT_FOUND);
    };

    if metadata.is_dir() {
        handle_directory_request(state, resolved_path, file_path).await
    } else {
        handle_file_request(resolved_path, headers).await
    }
}

// handle requests for files with range support
async fn handle_file_request(
    file_path: PathBuf,
    headers: HeaderMap,
) -> Result<Response, StatusCode> {
    info!("serving file: {}", file_path.display());

    let file_meta = match fs::open_file_for_serving(&file_path).await {
        Ok(meta) => meta,
        Err(err) => {
            error!("failed to open file {}: {}", file_path.display(), err);
            return Err(map_fs_error(&err));
        }
    };
    let file = file_meta.file;
    let file_size = file_meta.size;
    let mime_type = file_meta.mime_type;

    // check for range header
    if let Some(range_header) = headers.get(header::RANGE) {
        let range_str = match range_header.to_str() {
            Ok(s) => s,
            Err(_) => {
                warn!("invalid range header encoding");
                return Response::builder()
                    .status(StatusCode::RANGE_NOT_SATISFIABLE)
                    .header(header::CONTENT_RANGE, format!("bytes */{file_size}"))
                    .body(Body::empty())
                    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR);
            }
        };

        match parse_http_range(range_str) {
            Ok(parsed_ranges) => {
                match parsed_ranges.validate(file_size) {
                    Ok(valid_ranges) => {
                        if valid_ranges.is_empty() {
                            // no valid ranges, serve full file
                            serve_full_file(file, file_size, mime_type).await
                        } else {
                            // serve first range (we don't support multipart ranges)
                            let range = &valid_ranges[0];
                            let start = *range.start();
                            let end = *range.end();
                            info!(
                                "serving partial content: bytes {}-{}/{}",
                                start, end, file_size
                            );
                            serve_partial_file(file, start, end, file_size, mime_type).await
                        }
                    }
                    Err(_) => {
                        warn!("range not satisfiable after validation");
                        Response::builder()
                            .status(StatusCode::RANGE_NOT_SATISFIABLE)
                            .header(header::CONTENT_RANGE, format!("bytes */{file_size}"))
                            .body(Body::empty())
                            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
                    }
                }
            }
            Err(_) => {
                warn!("malformed range header");
                Response::builder()
                    .status(StatusCode::RANGE_NOT_SATISFIABLE)
                    .header(header::CONTENT_RANGE, format!("bytes */{file_size}"))
                    .body(Body::empty())
                    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
            }
        }
    } else {
        // no range header, serve full file
        serve_full_file(file, file_size, mime_type).await
    }
}

// serve partial content for range requests
async fn serve_partial_file(
    mut file: File,
    start: u64,
    end: u64,
    file_size: u64,
    mime_type: String,
) -> Result<Response, StatusCode> {
    // seek to start position
    file.seek(tokio::io::SeekFrom::Start(start))
        .await
        .map_err(|e| {
            error!("failed to seek file: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    // create limited stream for the range
    let take_bytes = end - start + 1;
    let limited_file = file.take(take_bytes);
    let stream = ReaderStream::new(limited_file);
    let body = Body::from_stream(stream);

    // build partial content response
    Response::builder()
        .status(StatusCode::PARTIAL_CONTENT)
        .header(header::CONTENT_TYPE, mime_type)
        .header(header::CONTENT_LENGTH, take_bytes)
        .header(header::ACCEPT_RANGES, "bytes")
        .header(
            header::CONTENT_RANGE,
            format!("bytes {start}-{end}/{file_size}"),
        )
        .body(body)
        .map_err(|e| {
            error!("failed to build partial response: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })
}

// serve the complete file
async fn serve_full_file(
    file: File,
    file_size: u64,
    mime_type: String,
) -> Result<Response, StatusCode> {
    let stream = ReaderStream::new(file);
    let body = Body::from_stream(stream);

    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, mime_type)
        .header(header::CONTENT_LENGTH, file_size)
        .header(header::ACCEPT_RANGES, "bytes")
        .body(body)
        .map_err(|e| {
            error!("failed to build response: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })
}

// handle requests for directories
async fn handle_directory_request(
    state: AppState,
    dir_path: PathBuf,
    request_path: String,
) -> Result<Response, StatusCode> {
    // ensure path ends with slash for directories
    if !request_path.ends_with('/') {
        info!("redirecting directory request to add trailing slash");
        return Response::builder()
            .status(StatusCode::MOVED_PERMANENTLY)
            .header(header::LOCATION, format!("{request_path}/"))
            .body(Body::empty())
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR);
    }

    // check for index files first
    const INDEX_FILES: &[&str] = &["index.html", "index.htm"];
    for index_file in INDEX_FILES {
        let index_path = dir_path.join(index_file);
        match tokio_fs::metadata(&index_path).await {
            Ok(metadata) => {
                if metadata.is_file() {
                    info!("serving index file: {}", index_path.display());
                    return handle_file_request(index_path, HeaderMap::new()).await;
                }
            }
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {}
            Err(err) => {
                error!(
                    "failed to read metadata for {}: {}",
                    index_path.display(),
                    err
                );
                return Err(map_io_error(&err));
            }
        }
    }

    // generate directory listing
    info!("serving directory listing: {}", dir_path.display());
    generate_directory_listing(&state, &dir_path, &request_path).await
}

// generate html directory listing
async fn generate_directory_listing(
    state: &AppState,
    dir_path: &StdPath,
    request_path: &str,
) -> Result<Response, StatusCode> {
    // collect directory entries
    let mut entries = fs::collect_directory_entries_filtered(
        dir_path,
        &state.config.server.public_dir,
        state.config.listing.ignore_file.as_ref(),
    )
    .await
    .map_err(|err| {
        error!("failed to read directory {}: {}", dir_path.display(), err);
        map_fs_error(&err)
    })?;

    listing::sort_entries(&mut entries);
    let html = listing::build_listing_html(&entries, request_path);

    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "text/html; charset=utf-8")
        .body(Body::from(html))
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

fn map_fs_error(err: &fs::FsError) -> StatusCode {
    match err {
        fs::FsError::InvalidPath(_) => StatusCode::BAD_REQUEST,
        fs::FsError::Io(io_err) => map_io_error(io_err),
    }
}

fn map_io_error(err: &std::io::Error) -> StatusCode {
    match err.kind() {
        ErrorKind::NotFound => StatusCode::NOT_FOUND,
        ErrorKind::PermissionDenied => StatusCode::FORBIDDEN,
        _ => StatusCode::INTERNAL_SERVER_ERROR,
    }
}
