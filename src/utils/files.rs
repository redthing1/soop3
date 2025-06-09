// file operations and formatting utilities

use std::path::Path;
use std::time::SystemTime;
use chrono::{DateTime, Local};
use tokio::fs;

/// metadata for a directory entry
#[derive(Debug, Clone)]
pub struct DirectoryEntry {
    pub name: String,
    pub size: u64,
    pub modified: SystemTime,
    pub is_dir: bool,
}

/// format file size in human-readable format (e.g., 1.5 KiB)
pub fn format_file_size(size: u64) -> String {
    const UNITS: &[&str] = &["B", "KiB", "MiB", "GiB", "TiB", "PiB"];
    const THRESHOLD: f64 = 1024.0;
    
    if size == 0 {
        return "0 B".to_string();
    }
    
    let mut size_f = size as f64;
    let mut unit_index = 0;
    
    while size_f >= THRESHOLD && unit_index < UNITS.len() - 1 {
        size_f /= THRESHOLD;
        unit_index += 1;
    }
    
    if unit_index == 0 {
        format!("{} {}", size, UNITS[unit_index])
    } else {
        format!("{:.1} {}", size_f, UNITS[unit_index])
    }
}

/// format a timestamp for display in directory listings
pub fn format_timestamp(timestamp: SystemTime) -> String {
    let datetime: DateTime<Local> = timestamp.into();
    datetime.format("%Y-%m-%d %H:%M:%S").to_string()
}

/// collect directory entries asynchronously
pub async fn collect_directory_entries(
    dir_path: &Path
) -> Result<Vec<DirectoryEntry>, std::io::Error> {
    let mut entries = Vec::new();
    let mut read_dir = fs::read_dir(dir_path).await?;
    
    while let Some(entry) = read_dir.next_entry().await? {
        let metadata = entry.metadata().await?;
        let file_name = entry.file_name();
        
        entries.push(DirectoryEntry {
            name: file_name.to_string_lossy().into_owned(),
            size: metadata.len(),
            modified: metadata.modified()?,
            is_dir: metadata.is_dir(),
        });
    }
    
    Ok(entries)
}

/// escape html special characters
pub fn escape_html(input: &str) -> String {
    input
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#x27;")
}

/// get mime type for a file based on its extension
pub fn get_mime_type(file_path: &Path) -> String {
    mime_guess::from_path(file_path)
        .first_or_octet_stream()
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_file_size_formatting() {
        assert_eq!(format_file_size(0), "0 B");
        assert_eq!(format_file_size(512), "512 B");
        assert_eq!(format_file_size(1024), "1.0 KiB");
        assert_eq!(format_file_size(1536), "1.5 KiB");
        assert_eq!(format_file_size(1048576), "1.0 MiB");
        assert_eq!(format_file_size(1073741824), "1.0 GiB");
    }
    
    #[test]
    fn test_html_escaping() {
        assert_eq!(escape_html("normal text"), "normal text");
        assert_eq!(escape_html("<script>"), "&lt;script&gt;");
        assert_eq!(escape_html("a & b"), "a &amp; b");
        assert_eq!(escape_html("\"quoted\""), "&quot;quoted&quot;");
    }
    
    #[test]
    fn test_mime_type_detection() {
        assert_eq!(get_mime_type(Path::new("file.html")), "text/html");
        assert_eq!(get_mime_type(Path::new("file.css")), "text/css");
        assert_eq!(get_mime_type(Path::new("file.js")), "text/javascript");
        assert_eq!(get_mime_type(Path::new("file.png")), "image/png");
        assert_eq!(get_mime_type(Path::new("file.jpg")), "image/jpeg");
    }
}