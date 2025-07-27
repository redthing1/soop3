// file serving request handlers

use axum::{
    body::Body,
    extract::{Path, State},
    http::{StatusCode, header},
    response::Response,
};
use std::path::{Path as StdPath, PathBuf};
use tokio::fs::File;
use tokio_util::io::ReaderStream;
use tracing::{debug, error, info, instrument, warn};

use super::assets::serve_embedded_favicon;
use crate::server::app::AppState;
use crate::utils::{
    files::{
        DirectoryEntry, collect_directory_entries, escape_html, format_file_size, format_timestamp,
        get_mime_type,
    },
    ignore::filter_with_ignore_patterns,
    paths::join_path_jailed,
};

/// handle root directory request
#[instrument(skip(state))]
pub async fn handle_root_request(State(state): State<AppState>) -> Result<Response, StatusCode> {
    handle_request_internal(state, "/".to_string()).await
}

/// main request handler - routes to file or directory handling  
#[instrument(skip(state), fields(path = %file_path))]
pub async fn handle_request(
    State(state): State<AppState>,
    Path(file_path): Path<String>,
) -> Result<Response, StatusCode> {
    // ensure path starts with / for consistency
    let normalized_path = if file_path.starts_with('/') {
        file_path
    } else {
        format!("/{file_path}")
    };
    handle_request_internal(state, normalized_path).await
}

/// internal request handling logic
async fn handle_request_internal(
    state: AppState,
    file_path: String,
) -> Result<Response, StatusCode> {
    info!("processing GET request");

    // validate and resolve path securely
    let resolved_path = match resolve_safe_path(&state.config.server.public_dir, &file_path) {
        Ok(path) => path,
        Err(e) => {
            warn!("rejecting request with bad path: {} - {}", file_path, e);
            return Err(StatusCode::BAD_REQUEST);
        }
    };

    debug!("resolved path: {}", resolved_path.display());

    // special case: favicon.ico
    if file_path.ends_with("/favicon.ico") && !resolved_path.exists() {
        info!("serving embedded favicon.ico");
        return serve_embedded_favicon().await;
    }

    // check if path exists
    if !resolved_path.exists() {
        error!("path does not exist: {}", resolved_path.display());
        return Err(StatusCode::NOT_FOUND);
    }

    if resolved_path.is_dir() {
        handle_directory_request(state, resolved_path, file_path).await
    } else {
        handle_file_request(resolved_path).await
    }
}

/// handle requests for files
async fn handle_file_request(file_path: PathBuf) -> Result<Response, StatusCode> {
    info!("serving file: {}", file_path.display());

    // open file
    let file = File::open(&file_path).await.map_err(|e| {
        error!("failed to open file {}: {}", file_path.display(), e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    // get file metadata
    let metadata = file.metadata().await.map_err(|e| {
        error!("failed to get file metadata {}: {}", file_path.display(), e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    // determine mime type
    let mime_type = get_mime_type(&file_path);

    // create streaming response
    let stream = ReaderStream::new(file);
    let body = Body::from_stream(stream);

    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, mime_type)
        .header(header::CONTENT_LENGTH, metadata.len())
        .body(body)
        .map_err(|e| {
            error!("failed to build response: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })
}

/// handle requests for directories
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
        if index_path.exists() && index_path.is_file() {
            info!("serving index file: {}", index_path.display());
            return handle_file_request(index_path).await;
        }
    }

    // generate directory listing
    info!("serving directory listing: {}", dir_path.display());
    generate_directory_listing(&state, &dir_path, &request_path).await
}

/// generate html directory listing
async fn generate_directory_listing(
    state: &AppState,
    dir_path: &StdPath,
    request_path: &str,
) -> Result<Response, StatusCode> {
    // collect directory entries
    let mut entries = collect_directory_entries(dir_path).await.map_err(|e| {
        error!("failed to read directory {}: {}", dir_path.display(), e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    // apply ignore file filtering if configured
    entries = match filter_with_ignore_patterns(
        entries,
        &state.config.server.public_dir,
        state.config.listing.ignore_file.as_ref(),
    ) {
        Ok(filtered) => filtered,
        Err(e) => {
            warn!(
                "ignore file filtering failed: {}, continuing without filtering",
                e
            );
            // we've already moved entries, so recreate the list
            collect_directory_entries(dir_path).await.map_err(|e| {
                error!("failed to re-read directory {}: {}", dir_path.display(), e);
                StatusCode::INTERNAL_SERVER_ERROR
            })?
        }
    };

    // sort entries (directories first, then alphabetical)
    let mut sorted_entries = entries;
    sorted_entries.sort_by(|a, b| match (a.is_dir, b.is_dir) {
        (true, false) => std::cmp::Ordering::Less,
        (false, true) => std::cmp::Ordering::Greater,
        _ => a.name.cmp(&b.name),
    });

    // generate html
    let html = build_listing_html(&sorted_entries, request_path)?;

    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "text/html; charset=utf-8")
        .body(Body::from(html))
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

/// build the html content for directory listing
fn build_listing_html(
    entries: &[DirectoryEntry],
    request_path: &str,
) -> Result<String, StatusCode> {
    let mut html = String::new();

    // html document structure
    html.push_str("<!DOCTYPE html>");
    html.push_str("<html><head>");
    html.push_str("<meta charset=\"utf-8\">");
    html.push_str("<meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">");
    html.push_str(&format!(
        "<meta name=\"generator\" content=\"soop3 v{}\">",
        env!("CARGO_PKG_VERSION")
    ));
    html.push_str("<link rel=\"icon\" href=\"/__soop_static/icon.svg\">");
    html.push_str(&format!(
        "<title>soop3 | {}</title>",
        escape_html(request_path)
    ));
    html.push_str("<link rel=\"stylesheet\" href=\"/__soop_static/style.css\">");
    html.push_str("</head><body>");

    // content structure
    html.push_str("<div class=\"wrapper\">");
    html.push_str("<main>");
    html.push_str(
        "<a href=\"/\"><img src=\"/__soop_static/icon.svg\" alt=\"logo\" class=\"logo-icon\"></a>",
    );
    html.push_str(&format!(
        "<h1 class=\"index-info\">Index of <code>{}</code></h1>",
        escape_html(request_path)
    ));

    // file listing table
    html.push_str("<table class=\"list\">");
    html.push_str("<tr><th>name</th><th>size</th><th>modified</th></tr>");

    // parent directory link
    if request_path != "/" {
        html.push_str("<tr><td><a href=\"../\">../</a></td><td></td><td></td></tr>");
    }

    // directory entries
    for entry in entries {
        let display_name = if entry.is_dir {
            format!("{}/", entry.name)
        } else {
            entry.name.clone()
        };

        let size_str = if entry.is_dir {
            String::new()
        } else {
            format_file_size(entry.size)
        };

        let entry_path = if request_path.ends_with('/') {
            format!("{}{}", request_path, entry.name)
        } else {
            format!("{}/{}", request_path, entry.name)
        };

        // add trailing slash for directories to avoid redirect
        let final_entry_path = if entry.is_dir && !entry_path.ends_with('/') {
            format!("{entry_path}/")
        } else {
            entry_path
        };

        html.push_str(&format!(
            "<tr><td><a href=\"{}\">{}</a></td><td>{}</td><td>{}</td></tr>",
            escape_html(&final_entry_path),
            escape_html(&display_name),
            escape_html(&size_str),
            format_timestamp(entry.modified)
        ));
    }

    html.push_str("</table>");
    html.push_str("</main>");
    html.push_str(&format!(
        "<footer><p>Generated by <code>soop3 v{}</code></p></footer>",
        env!("CARGO_PKG_VERSION")
    ));
    html.push_str("</div></body></html>");

    Ok(html)
}

// favicon handling moved to assets module

/// safely resolve a request path relative to the public directory
fn resolve_safe_path(
    public_dir: &std::path::Path,
    request_path: &str,
) -> Result<PathBuf, Box<dyn std::error::Error>> {
    // remove leading slash from request path
    let clean_path = request_path.strip_prefix('/').unwrap_or(request_path);

    // use our secure path joining function
    join_path_jailed(public_dir, clean_path).map_err(|e| Box::new(e) as Box<dyn std::error::Error>)
}
