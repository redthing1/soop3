// configuration integration tests

mod support;

use axum::http::StatusCode;
use support::{BOUNDARY, app, base_config, multipart_body, multipart_request};
use tempfile::TempDir;
use tower::ServiceExt;

use soop3::config::{Cli, ListingConfig, UploadConfig, load_configuration};
use std::fs;

#[test]
fn config_file_precedence_without_cli_flags() {
    let temp_dir = TempDir::new().unwrap();
    let public_dir = temp_dir.path().join("public");
    fs::create_dir_all(&public_dir).unwrap();

    let config_path = temp_dir.path().join("config.toml");
    fs::write(
        &config_path,
        format!(
            r#"
[server]
host = "127.0.0.1"
port = 9001
enable_upload = true
public_dir = "{}"
"#,
            public_dir.display()
        ),
    )
    .unwrap();

    let cli = Cli {
        public_dir: None,
        enable_upload: false,
        host: None,
        port: None,
        config_file: Some(config_path),
        verbose: 0,
        quiet: 0,
        cors: vec![],
    };

    let config = load_configuration(&cli).unwrap();
    assert_eq!(config.server.host, "127.0.0.1");
    assert_eq!(config.server.port, 9001);
    assert_eq!(config.server.public_dir, public_dir);
    assert!(config.server.enable_upload);
}

#[tokio::test]
async fn upload_config_applies_timestamp_prefix() {
    let temp_dir = TempDir::new().unwrap();
    let public_dir = temp_dir.path();

    let mut config = base_config(public_dir);
    config.server.enable_upload = true;
    config.upload = UploadConfig {
        prepend_timestamp: true,
        prevent_overwrite: false,
        max_request_size: 1024 * 1024,
        create_directories: false,
    };

    let app = app(config);
    let body = multipart_body(BOUNDARY, "test.txt", b"Content");
    let response = app
        .oneshot(multipart_request("/", BOUNDARY, body))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NO_CONTENT);

    let files: Vec<_> = fs::read_dir(public_dir).unwrap().collect();
    assert_eq!(files.len(), 1);
    let file_name = files[0].as_ref().unwrap().file_name();
    let file_name_str = file_name.to_string_lossy();

    assert!(file_name_str.len() > "test.txt".len());
    assert!(file_name_str.ends_with("_test.txt"));
    assert!(file_name_str.chars().take(8).all(|c| c.is_ascii_digit()));
}

#[tokio::test]
async fn custom_upload_directory_is_used() {
    let temp_dir = TempDir::new().unwrap();
    let public_dir = temp_dir.path().join("public");
    let upload_dir = temp_dir.path().join("uploads");

    fs::create_dir(&public_dir).unwrap();
    fs::create_dir(&upload_dir).unwrap();
    fs::write(public_dir.join("test.txt"), "content").unwrap();

    let mut config = base_config(&public_dir);
    config.server.upload_dir = Some(upload_dir.clone());
    config.server.enable_upload = true;
    config.upload = UploadConfig {
        prepend_timestamp: false,
        prevent_overwrite: false,
        ..Default::default()
    };

    let app = app(config);
    let body = multipart_body(BOUNDARY, "upload.txt", b"upload content");
    let response = app
        .oneshot(multipart_request("/", BOUNDARY, body))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NO_CONTENT);
    assert!(upload_dir.join("upload.txt").exists());
    assert!(!public_dir.join("upload.txt").exists());
}

#[tokio::test]
async fn listing_uses_custom_ignore_file() {
    let temp_dir = TempDir::new().unwrap();
    let public_dir = temp_dir.path();

    fs::write(public_dir.join("visible.txt"), "content").unwrap();
    fs::write(public_dir.join("hidden.log"), "log content").unwrap();
    fs::write(public_dir.join("custom.ignore"), "*.log\n").unwrap();

    let mut config = base_config(public_dir);
    config.listing = ListingConfig {
        ignore_file: Some("custom.ignore".into()),
    };

    let app = app(config);
    let response = app.oneshot(support::get("/")).await.unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = support::body_string(response).await;
    assert!(body.contains("visible.txt"));
    assert!(!body.contains("hidden.log"));
}

#[test]
fn validation_requires_complete_auth_credentials() {
    let temp_dir = TempDir::new().unwrap();
    let public_dir = temp_dir.path().join("public");
    fs::create_dir_all(&public_dir).unwrap();

    let config_path = temp_dir.path().join("config.toml");
    fs::write(
        &config_path,
        format!(
            r#"
[server]
public_dir = "{}"

[security]
username = "admin"
policy = "authenticate_all"
"#,
            public_dir.display()
        ),
    )
    .unwrap();

    let cli = Cli {
        public_dir: None,
        enable_upload: false,
        host: None,
        port: None,
        config_file: Some(config_path),
        verbose: 0,
        quiet: 0,
        cors: vec![],
    };

    let result = load_configuration(&cli);
    assert!(result.is_err());
}
