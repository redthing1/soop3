// path operations and security functions

use std::path::{Path, PathBuf, Component};
use percent_encoding::percent_decode_str;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum PathTraversalError {
    #[error("invalid base path")]
    InvalidBasePath,
    
    #[error("invalid target path")]
    InvalidTargetPath,
    
    #[error("path outside jail: base={base:?}, target={target:?}")]
    OutsideJail { base: PathBuf, target: PathBuf },
    
    #[error("invalid encoding in path")]
    InvalidEncoding,
    
    #[error("windows prefix not allowed")]
    WindowsPrefix,
}

/// safely join a path component to a base directory, preventing traversal attacks
/// this is the core security function that prevents directory traversal
pub fn join_path_jailed(
    base_dir: &Path,
    component: &str,
) -> Result<PathBuf, PathTraversalError> {
    // normalize component to prevent traversal
    let normalized = normalize_path_component(component)?;
    
    // join paths
    let joined = base_dir.join(normalized);
    
    // canonicalize base directory first
    let canonical_base = base_dir.canonicalize()
        .map_err(|_| PathTraversalError::InvalidBasePath)?;
    
    // try to canonicalize joined path, if it fails manually resolve it
    let canonical_joined = if joined.exists() {
        joined.canonicalize()
            .map_err(|_| PathTraversalError::InvalidTargetPath)?
    } else {
        // file doesn't exist yet, manually resolve from canonical base
        let relative_part = joined.strip_prefix(base_dir)
            .map_err(|_| PathTraversalError::InvalidTargetPath)?;
        canonical_base.join(relative_part)
    };
    
    // ensure result is within jail boundaries
    if !canonical_joined.starts_with(&canonical_base) {
        return Err(PathTraversalError::OutsideJail {
            base: canonical_base,
            target: canonical_joined,
        });
    }
    
    Ok(canonical_joined)
}

/// normalize a path component by url-decoding and cleaning up dangerous elements
fn normalize_path_component(component: &str) -> Result<PathBuf, PathTraversalError> {
    // url decode the component
    let decoded = percent_decode_str(component)
        .decode_utf8()
        .map_err(|_| PathTraversalError::InvalidEncoding)?;
    
    // build normalized path from components
    let mut normalized = PathBuf::new();
    
    for component in Path::new(decoded.as_ref()).components() {
        match component {
            Component::Normal(name) => normalized.push(name),
            Component::CurDir => {}, // ignore "."
            Component::ParentDir => {
                // allow going up, but validation will catch jail escapes
                normalized.push("..");
            },
            Component::RootDir => {
                // start fresh from root
                normalized = PathBuf::from("/");
            },
            Component::Prefix(_) => {
                // windows drive prefixes not allowed
                return Err(PathTraversalError::WindowsPrefix);
            },
        }
    }
    
    Ok(normalized)
}

/// manually resolve a path when canonicalize fails (e.g., for non-existent files)
#[allow(dead_code)]  
fn resolve_path_manually(base: &Path, target: &Path) -> PathBuf {
    let mut resolved = base.to_path_buf();
    
    // add each component of the relative part
    if let Ok(relative) = target.strip_prefix(base) {
        resolved.push(relative);
    } else {
        // fallback: use the target as-is
        return target.to_path_buf();
    }
    
    // normalize by handling .. and . components
    let mut components = Vec::new();
    for component in resolved.components() {
        match component {
            Component::ParentDir => {
                components.pop();
            },
            Component::CurDir => {
                // ignore
            },
            other => {
                components.push(other);
            },
        }
    }
    
    // rebuild path
    let mut result = PathBuf::new();
    for component in components {
        result.push(component);
    }
    
    result
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
}