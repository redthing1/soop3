// configuration and startup integration tests

use axum::body::Body;
use axum::http::{Request, StatusCode};
use soop3::{
    config::{
        AppConfig, ListingConfig, SecurityConfig, SecurityPolicy, ServerConfig, UploadConfig,
    },
    server::app::create_test_app,
};
use std::fs;
use std::path::PathBuf;
use tempfile::TempDir;
use tower::ServiceExt;

#[tokio::test]
async fn test_config_validation_errors() {
    // this test verifies that invalid configurations are handled properly
    // note: we can't easily test config validation in integration tests since
    // create_test_app bypasses validation, but we can test the behavior

    let temp_dir = TempDir::new().unwrap();
    let public_dir = temp_dir.path();

    // test config with inconsistent auth settings (username without password)
    let config = AppConfig {
        server: ServerConfig {
            public_dir: public_dir.to_path_buf(),
            ..Default::default()
        },
        security: SecurityConfig {
            username: Some("admin".to_string()),
            password: None, // missing password
            policy: SecurityPolicy::AuthenticateAll,
        },
        ..Default::default()
    };

    let app = create_test_app(config);

    // server should handle this gracefully - requests requiring auth should fail
    let response = app
        .oneshot(Request::builder().uri("/").body(Body::empty()).unwrap())
        .await
        .unwrap();

    // should get internal server error since auth is required but incomplete
    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
}

#[tokio::test]
async fn test_upload_config_variations() {
    let temp_dir = TempDir::new().unwrap();
    let public_dir = temp_dir.path();

    // test with timestamp prepending enabled
    let config = AppConfig {
        server: ServerConfig {
            public_dir: public_dir.to_path_buf(),
            enable_upload: true,
            ..Default::default()
        },
        security: SecurityConfig {
            policy: SecurityPolicy::AuthenticateNone,
            ..Default::default()
        },
        upload: UploadConfig {
            prepend_timestamp: true,
            prevent_overwrite: false,
            max_request_size: 1024 * 1024,
            create_directories: false,
        },
        ..Default::default()
    };

    let app = create_test_app(config);

    let boundary = "----WebKitFormBoundary7MA4YWxkTrZu0gW";
    let body = format!(
        "--{}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"test.txt\"\r\nContent-Type: text/plain\r\n\r\nContent\r\n--{}--\r\n",
        boundary, boundary
    );

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/")
                .header(
                    "content-type",
                    format!("multipart/form-data; boundary={}", boundary),
                )
                .body(Body::from(body))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NO_CONTENT);

    // find the uploaded file (should have timestamp prefix)
    let files: Vec<_> = fs::read_dir(public_dir).unwrap().collect();
    assert_eq!(files.len(), 1);
    let file_name = files[0].as_ref().unwrap().file_name();
    let file_name_str = file_name.to_string_lossy();

    // should have timestamp prefix format: YYYYMMDD_HHMMSS_test.txt
    assert!(file_name_str.len() > "test.txt".len());
    assert!(file_name_str.ends_with("_test.txt"));
    assert!(file_name_str.chars().take(8).all(|c| c.is_ascii_digit())); // YYYYMMDD
}

#[tokio::test]
async fn test_ignore_file_config_variations() {
    let temp_dir = TempDir::new().unwrap();
    let public_dir = temp_dir.path();

    // create test files
    fs::write(public_dir.join("visible.txt"), "content").unwrap();
    fs::write(public_dir.join("hidden.log"), "log content").unwrap();

    // create custom ignore file
    fs::write(public_dir.join("custom.ignore"), "*.log\n").unwrap();

    let config = AppConfig {
        server: ServerConfig {
            public_dir: public_dir.to_path_buf(),
            ..Default::default()
        },
        security: SecurityConfig {
            policy: SecurityPolicy::AuthenticateNone,
            ..Default::default()
        },
        listing: ListingConfig {
            ignore_file: Some(PathBuf::from("custom.ignore")),
        },
        ..Default::default()
    };

    let app = create_test_app(config);

    let response = app
        .oneshot(Request::builder().uri("/").body(Body::empty()).unwrap())
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let body_str = String::from_utf8(body.to_vec()).unwrap();

    // should contain visible file but not hidden file
    assert!(body_str.contains("visible.txt"));
    assert!(!body_str.contains("hidden.log"));
}

#[tokio::test]
async fn test_default_config_behavior() {
    let temp_dir = TempDir::new().unwrap();
    let public_dir = temp_dir.path();

    fs::write(public_dir.join("test.txt"), "content").unwrap();

    // test with completely default config
    let config = AppConfig {
        server: ServerConfig {
            public_dir: public_dir.to_path_buf(),
            ..Default::default()
        },
        ..Default::default()
    };

    let app = create_test_app(config);

    // should serve files without authentication (default policy is AuthenticateNone)
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/test.txt")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    // uploads should be disabled by default
    let boundary = "----test";
    let body = format!(
        "--{}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"upload.txt\"\r\n\r\ndata\r\n--{}--\r\n",
        boundary, boundary
    );

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/")
                .header(
                    "content-type",
                    format!("multipart/form-data; boundary={}", boundary),
                )
                .body(Body::from(body))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn test_custom_upload_directory() {
    let temp_dir = TempDir::new().unwrap();
    let public_dir = temp_dir.path().join("public");
    let upload_dir = temp_dir.path().join("uploads");

    fs::create_dir(&public_dir).unwrap();
    fs::create_dir(&upload_dir).unwrap();
    fs::write(public_dir.join("test.txt"), "content").unwrap();

    let config = AppConfig {
        server: ServerConfig {
            public_dir: public_dir.clone(),
            upload_dir: Some(upload_dir.clone()),
            enable_upload: true,
            ..Default::default()
        },
        security: SecurityConfig {
            policy: SecurityPolicy::AuthenticateNone,
            ..Default::default()
        },
        upload: UploadConfig {
            prepend_timestamp: false,
            prevent_overwrite: false,
            ..Default::default()
        },
        ..Default::default()
    };

    let app = create_test_app(config);

    // file serving should work from public_dir
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/test.txt")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    // uploads should go to upload_dir
    let boundary = "----test";
    let body = format!(
        "--{}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"upload.txt\"\r\n\r\nupload content\r\n--{}--\r\n",
        boundary, boundary
    );

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/")
                .header(
                    "content-type",
                    format!("multipart/form-data; boundary={}", boundary),
                )
                .body(Body::from(body))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NO_CONTENT);

    // file should be in upload_dir, not public_dir
    assert!(upload_dir.join("upload.txt").exists());
    assert!(!public_dir.join("upload.txt").exists());
    assert_eq!(
        fs::read_to_string(upload_dir.join("upload.txt")).unwrap(),
        "upload content"
    );
}
