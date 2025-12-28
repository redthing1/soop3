// ignore file pattern matching utilities

use anyhow::{Context, Result};
use regex::Regex;
use std::fs;
use std::path::{Path, PathBuf};
use tracing::warn;

/// convert a simple glob pattern to a regex string
/// supports * (any chars) and ? (single char) like soop2
pub fn pattern_to_regex(pattern: &str) -> Result<Regex> {
    let mut regex_pattern = String::new();

    // escape regex special characters except * and ?
    for ch in pattern.chars() {
        match ch {
            '*' => regex_pattern.push_str(".*"),
            '?' => regex_pattern.push('.'),
            '\\' | '^' | '$' | '.' | '+' | '(' | ')' | '[' | ']' | '{' | '}' | '|' => {
                regex_pattern.push('\\');
                regex_pattern.push(ch);
            }
            _ => regex_pattern.push(ch),
        }
    }

    // add start and end anchors for exact matching (soop2 behavior)
    if !regex_pattern.starts_with('^') {
        regex_pattern.insert(0, '^');
    }
    if !regex_pattern.ends_with('$') {
        regex_pattern.push('$');
    }

    Regex::new(&regex_pattern)
        .with_context(|| format!("failed to compile regex from pattern: {pattern}"))
}

/// read ignore patterns from a file
pub fn read_ignore_patterns(ignore_file: &Path) -> Result<Vec<Regex>> {
    let content = fs::read_to_string(ignore_file)
        .with_context(|| format!("failed to read ignore file: {}", ignore_file.display()))?;

    let patterns: Result<Vec<Regex>> = content
        .lines()
        .filter(|line| !line.trim().is_empty()) // skip empty lines like soop2
        .map(|line| pattern_to_regex(line.trim()))
        .collect();

    patterns.with_context(|| format!("failed to parse patterns from: {}", ignore_file.display()))
}

/// check if a relative path matches any of the ignore patterns
pub fn is_path_ignored(rel_path: &str, patterns: &[Regex]) -> bool {
    patterns.iter().any(|pattern| pattern.is_match(rel_path))
}

/// filter directory entries using ignore patterns
pub fn filter_with_ignore_patterns(
    entries: &[super::files::DirectoryEntry],
    base_dir: &Path,
    dir_path: &Path,
    ignore_file: Option<&PathBuf>,
) -> Result<Vec<super::files::DirectoryEntry>> {
    let canonical_base = base_dir
        .canonicalize()
        .unwrap_or_else(|_| base_dir.to_path_buf());

    // if no ignore file specified, return all entries
    let ignore_file = match ignore_file {
        Some(file) => file,
        None => return Ok(entries.to_vec()),
    };

    // resolve ignore file path relative to base directory
    let ignore_path = if ignore_file.is_absolute() {
        ignore_file.clone()
    } else {
        base_dir.join(ignore_file)
    };

    // if ignore file doesn't exist, return all entries (silent like soop2)
    match fs::metadata(&ignore_path) {
        Ok(_) => {}
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
            return Ok(entries.to_vec());
        }
        Err(err) => {
            warn!(
                "failed to read ignore file metadata for {}: {}",
                ignore_path.display(),
                err
            );
            return Ok(entries.to_vec());
        }
    }

    // read ignore patterns
    let patterns = match read_ignore_patterns(&ignore_path) {
        Ok(patterns) => patterns,
        Err(err) => {
            warn!(
                "failed to read ignore patterns from {}: {}",
                ignore_path.display(),
                err
            );
            return Ok(entries.to_vec());
        }
    };

    // filter entries
    let filtered: Vec<_> = entries
        .iter()
        .filter(|entry| {
            // create relative path from base_dir like soop2 does
            let entry_path = dir_path.join(&entry.name);
            let rel_path = match entry_path.strip_prefix(&canonical_base) {
                Ok(path) => path.to_string_lossy().to_string(),
                Err(_) => entry.name.clone(), // fallback to just the name
            };

            // check if path should be ignored
            !is_path_ignored(&rel_path, &patterns)
        })
        .cloned()
        .collect();

    Ok(filtered)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn test_pattern_to_regex() {
        // basic patterns
        let regex = pattern_to_regex("*.txt").unwrap();
        assert!(regex.is_match("file.txt"));
        assert!(regex.is_match("another.txt"));
        assert!(!regex.is_match("file.doc"));
        assert!(!regex.is_match("file.txtx")); // $ anchor should prevent this

        // single char wildcard
        let regex = pattern_to_regex("test?").unwrap();
        assert!(regex.is_match("test1"));
        assert!(regex.is_match("testa"));
        assert!(!regex.is_match("test"));
        assert!(!regex.is_match("test12"));

        // literal patterns
        let regex = pattern_to_regex("build").unwrap();
        assert!(regex.is_match("build"));
        assert!(!regex.is_match("builds"));
        assert!(!regex.is_match("rebuild"));

        // special chars should be escaped
        let regex = pattern_to_regex("file.log").unwrap();
        assert!(regex.is_match("file.log"));
        assert!(!regex.is_match("filexlog")); // . should be literal
    }

    #[test]
    fn test_ignore_file_reading() {
        let temp_dir = TempDir::new().unwrap();
        let ignore_file = temp_dir.path().join(".gitignore");

        fs::write(&ignore_file, "*.log\ntemp*\n\nbuild\n").unwrap();

        let patterns = read_ignore_patterns(&ignore_file).unwrap();
        assert_eq!(patterns.len(), 3); // empty line should be skipped

        assert!(is_path_ignored("debug.log", &patterns));
        assert!(is_path_ignored("temp123", &patterns));
        assert!(is_path_ignored("build", &patterns));
        assert!(!is_path_ignored("source.rs", &patterns));
    }

    #[test]
    fn test_filtering_directory_entries() {
        let temp_dir = TempDir::new().unwrap();
        let base_dir = temp_dir.path();
        let dir_path = base_dir;

        // create ignore file
        let ignore_file = base_dir.join(".gitignore");
        fs::write(&ignore_file, "*.log\ntemp*\nbuild\n").unwrap();

        // create test entries
        let entries = vec![
            super::super::files::DirectoryEntry {
                name: "source.rs".to_string(),
                size: 1000,
                modified: std::time::SystemTime::UNIX_EPOCH,
                is_dir: false,
            },
            super::super::files::DirectoryEntry {
                name: "debug.log".to_string(),
                size: 500,
                modified: std::time::SystemTime::UNIX_EPOCH,
                is_dir: false,
            },
            super::super::files::DirectoryEntry {
                name: "temp_file".to_string(),
                size: 200,
                modified: std::time::SystemTime::UNIX_EPOCH,
                is_dir: false,
            },
            super::super::files::DirectoryEntry {
                name: "build".to_string(),
                size: 0,
                modified: std::time::SystemTime::UNIX_EPOCH,
                is_dir: true,
            },
        ];

        let filtered = filter_with_ignore_patterns(
            &entries,
            base_dir,
            dir_path,
            Some(&PathBuf::from(".gitignore")),
        )
        .unwrap();

        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].name, "source.rs");
    }

    #[test]
    fn test_filtering_returns_entries_on_ignore_read_error() {
        let temp_dir = TempDir::new().unwrap();
        let base_dir = temp_dir.path();
        let dir_path = base_dir;

        let ignore_dir = base_dir.join("ignore_dir");
        fs::create_dir(&ignore_dir).unwrap();

        let entries = vec![
            super::super::files::DirectoryEntry {
                name: "source.rs".to_string(),
                size: 1000,
                modified: std::time::SystemTime::UNIX_EPOCH,
                is_dir: false,
            },
            super::super::files::DirectoryEntry {
                name: "debug.log".to_string(),
                size: 500,
                modified: std::time::SystemTime::UNIX_EPOCH,
                is_dir: false,
            },
        ];

        let filtered = filter_with_ignore_patterns(
            &entries,
            base_dir,
            dir_path,
            Some(&PathBuf::from("ignore_dir")),
        )
        .unwrap();

        assert_eq!(filtered.len(), entries.len());
        assert_eq!(filtered[0].name, "source.rs");
        assert_eq!(filtered[1].name, "debug.log");
    }
}
