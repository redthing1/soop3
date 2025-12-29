// path operations and security functions

use percent_encoding::{AsciiSet, CONTROLS, percent_decode_str, utf8_percent_encode};
use std::path::{Component, Path, PathBuf};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum PathTraversalError {
    #[error("invalid base path")]
    InvalidBasePath,

    #[error("invalid target path")]
    InvalidTargetPath,

    #[error("encoded slash not allowed in path")]
    EncodedSlash,

    #[error("path outside jail: base={base:?}, target={target:?}")]
    OutsideJail { base: PathBuf, target: PathBuf },

    #[error("invalid encoding in path")]
    InvalidEncoding,

    #[error("windows prefix not allowed")]
    WindowsPrefix,

    #[error("backslash not allowed in path")]
    Backslash,
}

const PATH_SEGMENT_ENCODE_SET: &AsciiSet = &CONTROLS
    .add(b' ')
    .add(b'"')
    .add(b'#')
    .add(b'%')
    .add(b'<')
    .add(b'>')
    .add(b'?')
    .add(b'`')
    .add(b'{')
    .add(b'}')
    .add(b'\\');

/// percent-encode each path segment for use in URL paths
pub fn encode_path_segments(path: &str) -> String {
    if path.is_empty() {
        return String::new();
    }

    let mut encoded = String::new();
    for (index, segment) in path.split('/').enumerate() {
        if index > 0 {
            encoded.push('/');
        }
        if segment.is_empty() {
            continue;
        }
        encoded.push_str(&utf8_percent_encode(segment, PATH_SEGMENT_ENCODE_SET).to_string());
    }

    encoded
}

/// safely join a path component to a base directory, preventing traversal attacks
/// this is the core security function that prevents directory traversal
pub fn join_path_jailed(base_dir: &Path, component: &str) -> Result<PathBuf, PathTraversalError> {
    join_path_jailed_follow_parents(base_dir, component)
}

/// resolve a jailed path while following symlinks in existing parent directories
pub fn join_path_jailed_follow_parents(
    base_dir: &Path,
    component: &str,
) -> Result<PathBuf, PathTraversalError> {
    let normalized = normalize_path_component(component)?;
    let canonical_base = base_dir
        .canonicalize()
        .map_err(|_| PathTraversalError::InvalidBasePath)?;

    let mut current = canonical_base.clone();

    for component in normalized.components() {
        match component {
            Component::Normal(name) => {
                let candidate = current.join(name);
                if candidate.exists() {
                    let canonical_candidate = candidate
                        .canonicalize()
                        .map_err(|_| PathTraversalError::InvalidTargetPath)?;
                    if !canonical_candidate.starts_with(&canonical_base) {
                        return Err(PathTraversalError::OutsideJail {
                            base: canonical_base,
                            target: canonical_candidate,
                        });
                    }
                    current = canonical_candidate;
                } else {
                    current = candidate;
                }
            }
            Component::CurDir => {}
            Component::ParentDir => {
                if !current.pop() || !current.starts_with(&canonical_base) {
                    return Err(PathTraversalError::OutsideJail {
                        base: canonical_base,
                        target: current,
                    });
                }
            }
            Component::RootDir => {
                return Err(PathTraversalError::InvalidTargetPath);
            }
            Component::Prefix(_) => {
                return Err(PathTraversalError::WindowsPrefix);
            }
        }
    }

    if !current.starts_with(&canonical_base) {
        return Err(PathTraversalError::OutsideJail {
            base: canonical_base,
            target: current,
        });
    }

    Ok(current)
}

/// normalize a path component by url-decoding and cleaning up dangerous elements
fn normalize_path_component(component: &str) -> Result<PathBuf, PathTraversalError> {
    if contains_encoded_slash(component) {
        return Err(PathTraversalError::EncodedSlash);
    }

    // url decode the component
    let decoded = percent_decode_str(component)
        .decode_utf8()
        .map_err(|_| PathTraversalError::InvalidEncoding)?;

    if decoded.contains('\0') {
        return Err(PathTraversalError::InvalidEncoding);
    }
    if decoded.contains('\\') {
        return Err(PathTraversalError::Backslash);
    }

    // build normalized path from components
    let mut normalized = PathBuf::new();

    for component in Path::new(decoded.as_ref()).components() {
        match component {
            Component::Normal(name) => normalized.push(name),
            Component::CurDir => {} // ignore "."
            Component::ParentDir => {
                // allow going up, but validation will catch jail escapes
                normalized.push("..");
            }
            Component::RootDir => {
                return Err(PathTraversalError::InvalidTargetPath);
            }
            Component::Prefix(_) => {
                // windows drive prefixes not allowed
                return Err(PathTraversalError::WindowsPrefix);
            }
        }
    }

    Ok(normalized)
}

fn contains_encoded_slash(component: &str) -> bool {
    let bytes = component.as_bytes();
    let mut index = 0;
    while index + 2 < bytes.len() {
        if bytes[index] == b'%' {
            let first = bytes[index + 1].to_ascii_lowercase();
            let second = bytes[index + 2].to_ascii_lowercase();
            if first == b'2' && second == b'f' {
                return true;
            }
        }
        index += 1;
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn test_safe_path_joining() {
        let temp_dir = TempDir::new().unwrap();
        let base_path = temp_dir.path();

        // create test file
        fs::write(base_path.join("test.txt"), "content").unwrap();

        // valid paths should succeed
        assert!(join_path_jailed(base_path, "test.txt").is_ok());

        // create subdirectory
        fs::create_dir(base_path.join("subdir")).unwrap();
        fs::write(base_path.join("subdir/nested.txt"), "content").unwrap();
        assert!(join_path_jailed(base_path, "subdir/nested.txt").is_ok());
    }

    #[test]
    fn test_path_traversal_prevention() {
        let temp_dir = TempDir::new().unwrap();
        let base_path = temp_dir.path();

        // create a file outside the jail
        let outside_file = temp_dir.path().parent().unwrap().join("outside.txt");
        fs::write(&outside_file, "secret").unwrap();

        // traversal attempts should fail
        assert!(join_path_jailed(base_path, "../outside.txt").is_err());
        assert!(join_path_jailed(base_path, "../../etc/passwd").is_err());
        assert!(join_path_jailed(base_path, "/etc/passwd").is_err());

        // encoded traversal attempts should also fail
        assert!(join_path_jailed(base_path, "%2e%2e/outside.txt").is_err());
        assert!(join_path_jailed(base_path, "..%2foutside.txt").is_err());
    }

    #[test]
    fn test_path_normalization() {
        assert!(normalize_path_component("normal_file.txt").is_ok());
        assert!(normalize_path_component("dir/file.txt").is_ok());
        assert!(normalize_path_component("../file.txt").is_ok()); // allowed, caught later

        // url encoding should be handled
        let result = normalize_path_component("file%20with%20spaces.txt").unwrap();
        assert_eq!(result, PathBuf::from("file with spaces.txt"));
    }

    #[test]
    fn test_encode_path_segments() {
        assert_eq!(
            encode_path_segments("/file with spaces.txt"),
            "/file%20with%20spaces.txt"
        );
        assert_eq!(encode_path_segments("/dir/file#1.txt"), "/dir/file%231.txt");
        assert_eq!(encode_path_segments("/"), "/");
    }
}
