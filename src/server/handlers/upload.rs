// file upload handlers

use std::path::PathBuf;
use axum::{
    extract::{Path, State, Multipart},
    response::Response,
    http::StatusCode,
    body::Body,
};
use chrono::Utc;
use tokio::fs;
use tracing::{info, warn, error, instrument};

use crate::server::app::AppState;
use crate::utils::paths::join_path_jailed;

/// handle file upload requests
#[instrument(skip(state, multipart), fields(path = %upload_path))]
pub async fn handle_upload_request(
    State(state): State<AppState>,
    Path(upload_path): Path<String>,
    mut multipart: Multipart,
) -> Result<Response, StatusCode> {
    info!("processing upload request");
    
    // verify uploads are enabled
    if !state.config.server.enable_upload {
        warn!("upload attempt but uploads are disabled");
        return Err(StatusCode::FORBIDDEN);
    }
    
    // extract uploaded file from multipart data
    let file_data = extract_upload_file(&mut multipart).await?;
    
    // validate and process upload
    let target_path = process_upload(
        &state.config,
        &upload_path,
        file_data,
    ).await?;
    
    info!("upload completed successfully: {}", target_path.display());
    
    // return success with no content
    Response::builder()
        .status(StatusCode::NO_CONTENT)
        .body(Body::empty())
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

/// extract file data from multipart form
async fn extract_upload_file(multipart: &mut Multipart) -> Result<UploadedFile, StatusCode> {
    let mut file_data = None;
    
    while let Some(field) = multipart.next_field().await
        .map_err(|e| {
            error!("failed to read multipart field: {}", e);
            StatusCode::BAD_REQUEST
        })?
    {
        let name = field.name().unwrap_or("").to_string();
        let filename = field.file_name().map(|s| s.to_string());
        
        info!("processing multipart field: {} (filename: {:?})", name, filename);
        
        if filename.is_some() {
            // this is a file field
            let data = field.bytes().await
                .map_err(|e| {
                    error!("failed to read file data: {}", e);
                    StatusCode::BAD_REQUEST
                })?;
            
            file_data = Some(UploadedFile {
                filename: filename.unwrap_or_else(|| "unnamed".to_string()),
                data: data.to_vec(),
            });
            break;
        }
    }
    
    file_data.ok_or_else(|| {
        warn!("no file found in upload request");
        StatusCode::BAD_REQUEST
    })
}

/// process and save uploaded file
async fn process_upload(
    config: &crate::config::AppConfig,
    upload_path: &str,
    file_data: UploadedFile,
) -> Result<PathBuf, StatusCode> {
    // determine target filename
    let filename = if config.upload.prepend_timestamp {
        let timestamp = Utc::now().format("%Y%m%d_%H%M%S");
        format!("{}_{}", timestamp, upload_path)
    } else {
        upload_path.to_string()
    };
    
    info!("target filename: {} (original: {})", filename, file_data.filename);
    
    // validate target path is within upload directory
    let target_path = join_path_jailed(config.upload_dir(), &filename)
        .map_err(|e| {
            error!("invalid upload path {}: {}", filename, e);
            StatusCode::BAD_REQUEST
        })?;
    
    // ensure parent directory exists
    if let Some(parent) = target_path.parent() {
        if !parent.exists() {
            if config.upload.create_directories {
                info!("creating directory: {}", parent.display());
                fs::create_dir_all(parent).await
                    .map_err(|e| {
                        error!("failed to create directory {}: {}", parent.display(), e);
                        StatusCode::INTERNAL_SERVER_ERROR
                    })?;
            } else {
                error!("directory does not exist: {}", parent.display());
                return Err(StatusCode::NOT_FOUND);
            }
        }
    }
    
    // check for existing file
    if target_path.exists() && config.upload.prevent_overwrite {
        error!("file already exists and overwrite is disabled: {}", target_path.display());
        return Err(StatusCode::CONFLICT);
    }
    
    if target_path.exists() {
        warn!("overwriting existing file: {}", target_path.display());
    }
    
    // validate file size
    if file_data.data.len() as u64 > config.upload.max_request_size {
        error!("file too large: {} bytes (max: {})", 
               file_data.data.len(), config.upload.max_request_size);
        return Err(StatusCode::PAYLOAD_TOO_LARGE);
    }
    
    // write file atomically
    write_upload_file(&target_path, &file_data.data).await?;
    
    Ok(target_path)
}

/// write uploaded file data to disk
async fn write_upload_file(target_path: &PathBuf, data: &[u8]) -> Result<(), StatusCode> {
    // write to temporary file first, then move to final location
    let temp_path = format!("{}.tmp", target_path.display());
    
    fs::write(&temp_path, data).await
        .map_err(|e| {
            error!("failed to write temporary file {}: {}", temp_path, e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    
    // atomic move to final location
    if let Err(e) = fs::rename(&temp_path, target_path).await {
        error!("failed to move file from {} to {}: {}", 
               temp_path, target_path.display(), e);
        // cleanup temp file
        let _ = fs::remove_file(&temp_path).await;
        return Err(StatusCode::INTERNAL_SERVER_ERROR);
    }
    
    info!("file written successfully: {}", target_path.display());
    Ok(())
}

/// uploaded file data
#[derive(Debug)]
struct UploadedFile {
    filename: String,
    data: Vec<u8>,
}