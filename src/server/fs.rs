// filesystem helpers for safe path resolution and directory reads

use std::path::{Path, PathBuf};
use thiserror::Error;
use tokio::fs::File;
use tracing::warn;

use crate::utils::{
    files::{DirectoryEntry, collect_directory_entries, get_mime_type},
    ignore::filter_with_ignore_patterns,
    paths::PathTraversalError,
    paths::join_path_jailed,
};

#[derive(Debug, Error)]
pub enum FsError {
    #[error("invalid path: {0}")]
    InvalidPath(#[from] PathTraversalError),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
}

pub struct FileMeta {
    pub file: File,
    pub size: u64,
    pub mime_type: String,
}

pub fn resolve_request_path(public_dir: &Path, request_path: &str) -> Result<PathBuf, FsError> {
    let clean_path = request_path.strip_prefix('/').unwrap_or(request_path);
    Ok(join_path_jailed(public_dir, clean_path)?)
}

pub async fn open_file_for_serving(file_path: &Path) -> Result<FileMeta, FsError> {
    let file = File::open(file_path).await?;
    let metadata = file.metadata().await?;
    let file_size = metadata.len();
    let mime_type = get_mime_type(file_path);

    Ok(FileMeta {
        file,
        size: file_size,
        mime_type,
    })
}

pub async fn collect_directory_entries_filtered(
    dir_path: &Path,
    public_dir: &Path,
    ignore_file: Option<&PathBuf>,
) -> Result<Vec<DirectoryEntry>, FsError> {
    let entries = collect_directory_entries(dir_path).await?;

    let Some(ignore_file) = ignore_file else {
        return Ok(entries);
    };

    match filter_with_ignore_patterns(&entries, public_dir, dir_path, Some(ignore_file)) {
        Ok(filtered) => Ok(filtered),
        Err(err) => {
            warn!(
                "ignore file filtering failed: {}, continuing without filtering",
                err
            );
            Ok(entries)
        }
    }
}
