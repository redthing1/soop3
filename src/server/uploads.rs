// upload processing and streaming helpers

use std::ffi::OsString;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use axum::{
    extract::multipart::{Field, MultipartError},
    http::StatusCode,
};
use chrono::Utc;
use thiserror::Error;
use tokio::fs;
use tokio::io::AsyncWriteExt;

use crate::config::AppConfig;
use crate::utils::paths::join_path_jailed;

const MAX_FILENAME_BYTES: usize = 255;

#[derive(Debug, Error)]
pub enum UploadError {
    #[error("invalid filename")]
    InvalidFilename,
    #[error("invalid upload path: {0}")]
    InvalidPath(#[from] crate::utils::paths::PathTraversalError),
    #[error("parent path is not a directory")]
    ParentNotDirectory,
    #[error("upload directory missing")]
    MissingDirectory,
    #[error("upload base is not a directory")]
    InvalidBase,
    #[error("file already exists")]
    Conflict,
    #[error("payload too large")]
    PayloadTooLarge,
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("multipart error: {0}")]
    Multipart(#[from] MultipartError),
}

impl UploadError {
    pub fn status_code(&self) -> StatusCode {
        match self {
            UploadError::InvalidFilename => StatusCode::BAD_REQUEST,
            UploadError::InvalidPath(_) => StatusCode::BAD_REQUEST,
            UploadError::ParentNotDirectory => StatusCode::CONFLICT,
            UploadError::MissingDirectory => StatusCode::NOT_FOUND,
            UploadError::InvalidBase => StatusCode::INTERNAL_SERVER_ERROR,
            UploadError::Conflict => StatusCode::CONFLICT,
            UploadError::PayloadTooLarge => StatusCode::PAYLOAD_TOO_LARGE,
            UploadError::Io(_) => StatusCode::INTERNAL_SERVER_ERROR,
            UploadError::Multipart(err) => err.status(),
        }
    }
}

pub async fn process_upload(
    config: &AppConfig,
    upload_path: &str,
    original_filename: String,
    field: Field<'_>,
) -> Result<PathBuf, UploadError> {
    ensure_upload_base_dir(config).await?;
    let sanitized_filename = sanitize_filename(&original_filename)?;
    let encoded_filename = escape_percent_for_join(&sanitized_filename);

    // determine target filename - combine upload path with multipart filename
    let base_filename = if upload_path.is_empty() {
        encoded_filename.clone()
    } else {
        // for directory paths, append the multipart filename
        let normalized_path = upload_path.trim_end_matches('/');
        if normalized_path.is_empty() {
            encoded_filename.clone()
        } else {
            format!("{normalized_path}/{encoded_filename}")
        }
    };

    let (filename, filename_for_validation) = if config.upload.prepend_timestamp {
        let timestamp = Utc::now().format("%Y%m%d_%H%M%S");
        (
            format!("{timestamp}_{base_filename}"),
            format!("{timestamp}_{sanitized_filename}"),
        )
    } else {
        (base_filename, sanitized_filename.clone())
    };

    validate_final_component(&filename_for_validation)?;

    // validate target path is within upload directory
    let target_path = join_path_jailed(config.upload_dir(), &filename)?;

    // ensure parent directory exists
    if let Some(parent) = target_path.parent() {
        match fs::metadata(parent).await {
            Ok(metadata) => {
                if !metadata.is_dir() {
                    return Err(UploadError::ParentNotDirectory);
                }
            }
            Err(err) if err.kind() == ErrorKind::NotFound => {
                if config.upload.create_directories {
                    fs::create_dir_all(parent).await?;
                } else {
                    return Err(UploadError::MissingDirectory);
                }
            }
            Err(err) => return Err(UploadError::Io(err)),
        }
    }

    // write file atomically with streaming
    write_multipart_field_streaming(
        field,
        &target_path,
        config.upload.max_request_size,
        config.upload.prevent_overwrite,
    )
    .await?;

    Ok(target_path)
}

async fn ensure_upload_base_dir(config: &AppConfig) -> Result<(), UploadError> {
    let upload_base = config.upload_dir();

    match fs::metadata(upload_base).await {
        Ok(metadata) => {
            if !metadata.is_dir() {
                return Err(UploadError::InvalidBase);
            }
            Ok(())
        }
        Err(err) if err.kind() == ErrorKind::NotFound => {
            if config.upload.create_directories {
                fs::create_dir_all(upload_base).await?;
                Ok(())
            } else {
                Err(UploadError::MissingDirectory)
            }
        }
        Err(err) => Err(UploadError::Io(err)),
    }
}

fn sanitize_filename(filename: &str) -> Result<String, UploadError> {
    if filename.is_empty() {
        return Err(UploadError::InvalidFilename);
    }

    if filename.len() > MAX_FILENAME_BYTES {
        return Err(UploadError::InvalidFilename);
    }

    if filename.contains('/') || filename.contains('\\') {
        return Err(UploadError::InvalidFilename);
    }

    Ok(filename.to_string())
}

fn validate_final_component(path: &str) -> Result<(), UploadError> {
    let file_name = Path::new(path)
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or(UploadError::InvalidFilename)?;

    if file_name == "." || file_name == ".." {
        return Err(UploadError::InvalidFilename);
    }

    if file_name.len() > MAX_FILENAME_BYTES {
        return Err(UploadError::InvalidFilename);
    }

    Ok(())
}

fn escape_percent_for_join(value: &str) -> String {
    if value.contains('%') {
        value.replace('%', "%25")
    } else {
        value.to_string()
    }
}

async fn write_multipart_field_streaming(
    mut field: Field<'_>,
    target_path: &Path,
    max_bytes: u64,
    prevent_overwrite: bool,
) -> Result<u64, UploadError> {
    if prevent_overwrite {
        let open_result = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(target_path)
            .await;
        let mut file = match open_result {
            Ok(file) => file,
            Err(err) if err.kind() == ErrorKind::AlreadyExists => {
                return Err(UploadError::Conflict);
            }
            Err(err) => return Err(UploadError::Io(err)),
        };

        let result = write_field_to_file(&mut field, &mut file, max_bytes).await;
        if result.is_err() {
            let _ = fs::remove_file(target_path).await;
        }
        return result;
    }

    let temp_path = temp_path_for_target(target_path);
    let mut file = fs::File::create(&temp_path).await?;

    let written = match write_field_to_file(&mut field, &mut file, max_bytes).await {
        Ok(written) => written,
        Err(err) => {
            let _ = fs::remove_file(&temp_path).await;
            return Err(err);
        }
    };

    drop(file);

    if let Err(err) = fs::rename(&temp_path, target_path).await {
        let _ = fs::remove_file(&temp_path).await;
        return Err(UploadError::Io(err));
    }

    Ok(written)
}

fn temp_path_for_target(target_path: &Path) -> PathBuf {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let mut file_name = target_path
        .file_name()
        .map(|name| name.to_os_string())
        .unwrap_or_else(|| OsString::from("upload"));
    file_name.push(format!(".{unique}.tmp"));
    target_path.with_file_name(file_name)
}

async fn write_field_to_file(
    field: &mut Field<'_>,
    file: &mut fs::File,
    max_bytes: u64,
) -> Result<u64, UploadError> {
    let mut written: u64 = 0;
    while let Some(chunk) = field.chunk().await? {
        written += chunk.len() as u64;
        if written > max_bytes {
            return Err(UploadError::PayloadTooLarge);
        }

        file.write_all(&chunk).await?;
    }

    file.flush().await?;
    Ok(written)
}
