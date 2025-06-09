// comprehensive unit tests for all soop3 modules

use soop3::{
    config::{load_config_from_file, AppConfig, SecurityConfig, SecurityPolicy, ServerConfig},
    utils::{
        files::{escape_html, format_file_size, format_timestamp, get_mime_type},
        paths::{join_path_jailed, PathTraversalError},
    },
};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::SystemTime;
use tempfile::{NamedTempFile, TempDir};

#[cfg(test)]
mod utils_tests {
    use super::*;

    mod file_utils_tests {
        use super::*;

        #[test]
        fn test_file_size_formatting() {
            assert_eq!(format_file_size(0), "0 B");
            assert_eq!(format_file_size(512), "512 B");
            assert_eq!(format_file_size(1024), "1.0 KiB");
            assert_eq!(format_file_size(1536), "1.5 KiB");
            assert_eq!(format_file_size(1048576), "1.0 MiB");
            assert_eq!(format_file_size(2147483648), "2.0 GiB");
            assert_eq!(format_file_size(1099511627776), "1.0 TiB");
        }

        #[test]
        fn test_html_escaping() {
            assert_eq!(escape_html("normal text"), "normal text");
            assert_eq!(
                escape_html("<script>alert('xss')</script>"),
                "&lt;script&gt;alert(&#x27;xss&#x27;)&lt;/script&gt;"
            );
            assert_eq!(escape_html("a & b"), "a &amp; b");
            assert_eq!(escape_html("\"quoted\""), "&quot;quoted&quot;");
            assert_eq!(escape_html("'single quotes'"), "&#x27;single quotes&#x27;");
            assert_eq!(escape_html("<>&\"'"), "&lt;&gt;&amp;&quot;&#x27;");
        }

        #[test]
        fn test_mime_type_detection() {
            assert_eq!(get_mime_type(Path::new("file.html")), "text/html");
            assert_eq!(get_mime_type(Path::new("file.css")), "text/css");
            assert_eq!(get_mime_type(Path::new("file.js")), "text/javascript");
            assert_eq!(get_mime_type(Path::new("file.png")), "image/png");
            assert_eq!(get_mime_type(Path::new("file.jpg")), "image/jpeg");
            assert_eq!(get_mime_type(Path::new("file.gif")), "image/gif");
            assert_eq!(get_mime_type(Path::new("file.svg")), "image/svg+xml");
            assert_eq!(get_mime_type(Path::new("file.pdf")), "application/pdf");
            assert_eq!(get_mime_type(Path::new("file.zip")), "application/zip");
            // unknown extensions should default to octet-stream
            assert_eq!(
                get_mime_type(Path::new("file.unknown")),
                "application/octet-stream"
            );
        }

        #[test]
        fn test_timestamp_formatting() {
            let timestamp = SystemTime::UNIX_EPOCH;
            let formatted = format_timestamp(timestamp);
            // should be in format YYYY-MM-DD HH:MM:SS
            assert!(formatted.len() == 19);
            assert!(formatted.contains("-"));
            assert!(formatted.contains(":"));
            assert!(formatted.contains(" "));
        }
    }

    mod path_security_tests {
        use super::*;

        #[test]
        fn test_safe_path_joining() {
            let temp_dir = TempDir::new().unwrap();
            let base_path = temp_dir.path();

            // create test files
            fs::write(base_path.join("test.txt"), "content").unwrap();
            fs::create_dir(base_path.join("subdir")).unwrap();
            fs::write(base_path.join("subdir/nested.txt"), "content").unwrap();

            // valid paths should succeed
            assert!(join_path_jailed(base_path, "test.txt").is_ok());
            assert!(join_path_jailed(base_path, "subdir/nested.txt").is_ok());
            assert!(join_path_jailed(base_path, "subdir").is_ok());

            // test that returned paths are correct
            let result = join_path_jailed(base_path, "test.txt").unwrap();
            assert!(result.exists());
            // canonicalize both paths for comparison (handles symlinks on macOS)
            let canonical_base = base_path.canonicalize().unwrap();
            assert!(result.starts_with(&canonical_base));
        }

        #[test]
        fn test_path_traversal_prevention() {
            let temp_dir = TempDir::new().unwrap();
            let base_path = temp_dir.path();

            // create a file outside the jail
            let outside_file = temp_dir.path().parent().unwrap().join("outside.txt");
            fs::write(&outside_file, "secret").unwrap();

            // various traversal attempts should fail
            assert!(join_path_jailed(base_path, "../outside.txt").is_err());
            assert!(join_path_jailed(base_path, "../../etc/passwd").is_err());
            assert!(join_path_jailed(base_path, "/etc/passwd").is_err());

            // windows-style paths with backslashes (only test on windows)
            #[cfg(windows)]
            assert!(join_path_jailed(base_path, "\\..\\outside.txt").is_err());

            // encoded traversal attempts should also fail
            assert!(join_path_jailed(base_path, "%2e%2e/outside.txt").is_err());
            assert!(join_path_jailed(base_path, "..%2foutside.txt").is_err());
            assert!(join_path_jailed(base_path, "%2e%2e%2foutside.txt").is_err());

            // nested traversal attempts
            assert!(join_path_jailed(base_path, "sub/../../../etc/passwd").is_err());
            assert!(join_path_jailed(base_path, "././../outside.txt").is_err());
        }

        #[test]
        fn test_path_normalization() {
            let temp_dir = TempDir::new().unwrap();
            let base_path = temp_dir.path();

            // create test structure
            fs::create_dir(base_path.join("sub")).unwrap();
            fs::create_dir(base_path.join("other")).unwrap();
            fs::write(base_path.join("sub/file.txt"), "content").unwrap();

            // these should all resolve to the same file
            let normal = join_path_jailed(base_path, "sub/file.txt").unwrap();
            let with_dot = join_path_jailed(base_path, "sub/./file.txt").unwrap();
            let with_redundant = join_path_jailed(base_path, "other/../sub/file.txt").unwrap();

            assert_eq!(normal.file_name(), Some(std::ffi::OsStr::new("file.txt")));
            assert_eq!(with_dot.file_name(), Some(std::ffi::OsStr::new("file.txt")));
            assert_eq!(
                with_redundant.file_name(),
                Some(std::ffi::OsStr::new("file.txt"))
            );

            // test a path that should fail due to traversal
            let traversal_attempt = join_path_jailed(base_path, "sub/../../outside.txt");
            assert!(traversal_attempt.is_err());
        }

        #[test]
        fn test_url_encoded_paths() {
            let temp_dir = TempDir::new().unwrap();
            let base_path = temp_dir.path();

            // create file with spaces
            fs::write(base_path.join("file with spaces.txt"), "content").unwrap();

            // url encoded version should work
            let result = join_path_jailed(base_path, "file%20with%20spaces.txt").unwrap();
            assert!(result.exists());
            assert_eq!(
                result.file_name(),
                Some(std::ffi::OsStr::new("file with spaces.txt"))
            );
        }

        #[test]
        fn test_non_existent_file_handling() {
            let temp_dir = TempDir::new().unwrap();
            let base_path = temp_dir.path();

            // non-existent files should still be jailed properly
            let result = join_path_jailed(base_path, "nonexistent.txt").unwrap();
            let canonical_base = base_path.canonicalize().unwrap();
            assert!(result.starts_with(&canonical_base));
            assert_eq!(
                result.file_name(),
                Some(std::ffi::OsStr::new("nonexistent.txt"))
            );

            // but traversal attempts on non-existent files should still fail
            assert!(join_path_jailed(base_path, "../nonexistent.txt").is_err());
        }

        #[test]
        fn test_error_types() {
            let temp_dir = TempDir::new().unwrap();
            let nonexistent_base = temp_dir.path().join("nonexistent");

            // invalid base path should return InvalidBasePath
            let result = join_path_jailed(&nonexistent_base, "file.txt");
            assert!(matches!(result, Err(PathTraversalError::InvalidBasePath)));

            // test that error messages are informative
            let err = join_path_jailed(temp_dir.path(), "../outside.txt").unwrap_err();
            assert!(matches!(err, PathTraversalError::OutsideJail { .. }));

            // test invalid encoding (use actually invalid UTF-8 sequence)
            let result = join_path_jailed(temp_dir.path(), "file%C0%C1.txt");
            assert!(matches!(result, Err(PathTraversalError::InvalidEncoding)));
        }
    }
}

#[cfg(test)]
mod config_tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = AppConfig::default();

        assert_eq!(config.server.host, "0.0.0.0");
        assert_eq!(config.server.port, 8000);
        assert!(!config.server.enable_upload);
        assert_eq!(config.server.public_dir, PathBuf::from("."));

        assert!(config.security.username.is_none());
        assert!(config.security.password.is_none());
        assert_eq!(config.security.policy, SecurityPolicy::AuthenticateNone);

        assert!(config.upload.prepend_timestamp); // default is true in soop2
        assert!(config.upload.prevent_overwrite);
        assert_eq!(config.upload.max_request_size, 1024 * 1024 * 1024); // 1 GiB like soop2
    }

    #[test]
    fn test_security_policy_parsing() {
        // test string parsing
        assert_eq!(
            "authenticate_none".parse::<SecurityPolicy>().unwrap(),
            SecurityPolicy::AuthenticateNone
        );
        assert_eq!(
            "authenticate_upload".parse::<SecurityPolicy>().unwrap(),
            SecurityPolicy::AuthenticateUpload
        );
        assert_eq!(
            "authenticate_all".parse::<SecurityPolicy>().unwrap(),
            SecurityPolicy::AuthenticateAll
        );

        // test case insensitivity
        assert_eq!(
            "AUTHENTICATE_ALL".parse::<SecurityPolicy>().unwrap(),
            SecurityPolicy::AuthenticateAll
        );
        assert_eq!(
            "Authenticate_Upload".parse::<SecurityPolicy>().unwrap(),
            SecurityPolicy::AuthenticateUpload
        );

        // test invalid values
        assert!("invalid".parse::<SecurityPolicy>().is_err());
        assert!("".parse::<SecurityPolicy>().is_err());
    }

    #[test]
    fn test_config_file_parsing() {
        let config_content = r#"
[server]
host = "0.0.0.0"
port = 9000
enable_upload = true
public_dir = "/var/www"

[security]
username = "admin"
password = "secret"
policy = "authenticate_upload"

[upload]
prepend_timestamp = true
prevent_overwrite = false
max_request_size = 100000000

[listing]
# no specific settings needed for now
"#;

        let mut temp_file = NamedTempFile::new().unwrap();
        std::io::Write::write_all(&mut temp_file, config_content.as_bytes()).unwrap();

        let config = load_config_from_file(temp_file.path()).unwrap();

        assert_eq!(config.server.host, "0.0.0.0");
        assert_eq!(config.server.port, 9000);
        assert!(config.server.enable_upload);
        assert_eq!(config.server.public_dir, PathBuf::from("/var/www"));

        assert_eq!(config.security.username, Some("admin".to_string()));
        assert_eq!(config.security.password, Some("secret".to_string()));
        assert_eq!(config.security.policy, SecurityPolicy::AuthenticateUpload);

        assert!(config.upload.prepend_timestamp);
        assert!(!config.upload.prevent_overwrite);
        assert_eq!(config.upload.max_request_size, 100000000);
    }

    #[test]
    fn test_upload_dir_calculation() {
        let config = AppConfig {
            server: ServerConfig {
                public_dir: PathBuf::from("/var/www"),
                ..Default::default()
            },
            ..Default::default()
        };

        assert_eq!(config.upload_dir(), &PathBuf::from("/var/www"));
    }
}

#[cfg(test)]
mod authentication_tests {
    use super::*;
    use soop3::server::middleware::auth::{parse_basic_auth, validate_credentials, Credentials};

    #[test]
    fn test_basic_auth_parsing() {
        // valid basic auth header
        let header = "Basic dGVzdDp0ZXN0"; // test:test in base64
        let credentials = parse_basic_auth(header).unwrap();
        assert_eq!(credentials.username, "test");
        assert_eq!(credentials.password, "test");

        // test with special characters
        let header = "Basic dXNlcjpwYXNzQDEyMw=="; // user:pass@123 in base64
        let credentials = parse_basic_auth(header).unwrap();
        assert_eq!(credentials.username, "user");
        assert_eq!(credentials.password, "pass@123");

        // invalid format
        assert!(parse_basic_auth("Bearer token").is_err());
        assert!(parse_basic_auth("Basic").is_err());
        assert!(parse_basic_auth("Basic invalid-base64").is_err());
        assert!(parse_basic_auth("Basic dGVzdA==").is_err()); // missing colon
    }

    #[test]
    fn test_credential_validation() {
        let security_config = SecurityConfig {
            username: Some("admin".to_string()),
            password: Some("secret".to_string()),
            policy: SecurityPolicy::AuthenticateAll,
        };

        // correct credentials
        let valid_creds = Credentials {
            username: "admin".to_string(),
            password: "secret".to_string(),
        };
        assert!(validate_credentials(&security_config, &valid_creds));

        // wrong username
        let invalid_user = Credentials {
            username: "wrong".to_string(),
            password: "secret".to_string(),
        };
        assert!(!validate_credentials(&security_config, &invalid_user));

        // wrong password
        let invalid_pass = Credentials {
            username: "admin".to_string(),
            password: "wrong".to_string(),
        };
        assert!(!validate_credentials(&security_config, &invalid_pass));

        // empty credentials
        let empty_creds = Credentials {
            username: "".to_string(),
            password: "".to_string(),
        };
        assert!(!validate_credentials(&security_config, &empty_creds));
    }

    #[test]
    fn test_constant_time_comparison() {
        // this test ensures our constant-time comparison works
        // it's hard to test timing, but we can test correctness
        use soop3::server::middleware::auth::constant_time_eq;

        assert!(constant_time_eq(b"hello", b"hello"));
        assert!(!constant_time_eq(b"hello", b"world"));
        assert!(!constant_time_eq(b"hello", b"hell"));
        assert!(!constant_time_eq(b"hell", b"hello"));
        assert!(!constant_time_eq(b"", b"hello"));
        assert!(constant_time_eq(b"", b""));
    }
}
