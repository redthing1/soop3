// upload handling behavior

mod support;

use axum::http::StatusCode;
use support::{BOUNDARY, app, base_config, multipart_body, multipart_request, upload_config};
use tempfile::TempDir;
use tower::ServiceExt;

use std::fs;

#[tokio::test]
async fn uploads_basic_file() {
    let temp_dir = TempDir::new().unwrap();
    let public_dir = temp_dir.path();

    let config = upload_config(
        public_dir,
        soop3::config::UploadConfig {
            prepend_timestamp: false,
            prevent_overwrite: false,
            ..Default::default()
        },
    );

    let app = app(config);
    let body = multipart_body(BOUNDARY, "upload.txt", b"Uploaded content");
    let response = app
        .oneshot(multipart_request("/", BOUNDARY, body))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NO_CONTENT);
    assert!(public_dir.join("upload.txt").exists());
    assert_eq!(
        fs::read_to_string(public_dir.join("upload.txt")).unwrap(),
        "Uploaded content"
    );
}

#[tokio::test]
async fn upload_disabled_is_forbidden() {
    let temp_dir = TempDir::new().unwrap();
    let public_dir = temp_dir.path();

    let config = base_config(public_dir);
    let app = app(config);

    let body = multipart_body(BOUNDARY, "upload.txt", b"Content");
    let response = app
        .oneshot(multipart_request("/", BOUNDARY, body))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    assert!(!public_dir.join("upload.txt").exists());
}

#[tokio::test]
async fn upload_prevent_overwrite_rejects_second_write() {
    let temp_dir = TempDir::new().unwrap();
    let public_dir = temp_dir.path();

    let config = upload_config(
        public_dir,
        soop3::config::UploadConfig {
            prepend_timestamp: false,
            prevent_overwrite: true,
            ..Default::default()
        },
    );

    let app = app(config);

    let body = multipart_body(BOUNDARY, "upload.txt", b"first");
    let response = app
        .clone()
        .oneshot(multipart_request("/", BOUNDARY, body))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::NO_CONTENT);

    let body = multipart_body(BOUNDARY, "upload.txt", b"second");
    let response = app
        .oneshot(multipart_request("/", BOUNDARY, body))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::CONFLICT);
    assert_eq!(
        fs::read_to_string(public_dir.join("upload.txt")).unwrap(),
        "first"
    );
}

#[tokio::test]
async fn upload_allows_overwrite_when_enabled() {
    let temp_dir = TempDir::new().unwrap();
    let public_dir = temp_dir.path();

    let config = upload_config(
        public_dir,
        soop3::config::UploadConfig {
            prepend_timestamp: false,
            prevent_overwrite: false,
            ..Default::default()
        },
    );

    let app = app(config);

    let body = multipart_body(BOUNDARY, "upload.txt", b"first");
    let response = app
        .clone()
        .oneshot(multipart_request("/", BOUNDARY, body))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::NO_CONTENT);

    let body = multipart_body(BOUNDARY, "upload.txt", b"second");
    let response = app
        .oneshot(multipart_request("/", BOUNDARY, body))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::NO_CONTENT);
    assert_eq!(
        fs::read_to_string(public_dir.join("upload.txt")).unwrap(),
        "second"
    );
}

#[tokio::test]
async fn upload_rejects_path_with_file_component() {
    let temp_dir = TempDir::new().unwrap();
    let public_dir = temp_dir.path();

    fs::write(public_dir.join("file_component"), "not a directory").unwrap();

    let config = upload_config(
        public_dir,
        soop3::config::UploadConfig {
            prepend_timestamp: false,
            prevent_overwrite: false,
            create_directories: true,
            ..Default::default()
        },
    );

    let app = app(config);
    let body = multipart_body(BOUNDARY, "upload.txt", b"content");
    let response = app
        .oneshot(multipart_request("/file_component/nested/", BOUNDARY, body))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::CONFLICT);
    assert!(!public_dir.join("file_component/nested/upload.txt").exists());
}

#[tokio::test]
async fn upload_rejects_parent_path_that_is_a_file() {
    let temp_dir = TempDir::new().unwrap();
    let public_dir = temp_dir.path();

    fs::write(public_dir.join("subdir"), "not a dir").unwrap();

    let config = upload_config(
        public_dir,
        soop3::config::UploadConfig {
            prepend_timestamp: false,
            prevent_overwrite: false,
            ..Default::default()
        },
    );

    let app = app(config);
    let body = multipart_body(BOUNDARY, "upload.txt", b"content");
    let response = app
        .oneshot(multipart_request("/subdir/", BOUNDARY, body))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::CONFLICT);
    assert!(!public_dir.join("subdir").join("upload.txt").exists());
}

#[tokio::test]
async fn upload_allows_percent_encoded_filename_literal() {
    let temp_dir = TempDir::new().unwrap();
    let public_dir = temp_dir.path();

    let config = upload_config(
        public_dir,
        soop3::config::UploadConfig {
            prepend_timestamp: false,
            prevent_overwrite: false,
            ..Default::default()
        },
    );

    let app = app(config);
    let body = multipart_body(BOUNDARY, "%2F.txt", b"percent content");
    let response = app
        .oneshot(multipart_request("/", BOUNDARY, body))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NO_CONTENT);
    assert!(public_dir.join("%2F.txt").exists());
    assert_eq!(
        fs::read_to_string(public_dir.join("%2F.txt")).unwrap(),
        "percent content"
    );
}

#[tokio::test]
async fn upload_respects_size_limits() {
    let temp_dir = TempDir::new().unwrap();
    let public_dir = temp_dir.path();

    let config = upload_config(
        public_dir,
        soop3::config::UploadConfig {
            prepend_timestamp: false,
            prevent_overwrite: false,
            max_request_size: 1024,
            ..Default::default()
        },
    );

    let app = app(config);
    let large_content = vec![b'x'; 2000];
    let body = multipart_body(BOUNDARY, "large.txt", &large_content);
    let response = app
        .oneshot(multipart_request("/", BOUNDARY, body))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::PAYLOAD_TOO_LARGE);
    assert!(!public_dir.join("large.txt").exists());
}

#[tokio::test]
async fn upload_to_subdirectory_creates_path() {
    let temp_dir = TempDir::new().unwrap();
    let public_dir = temp_dir.path();

    let config = upload_config(
        public_dir,
        soop3::config::UploadConfig {
            prepend_timestamp: false,
            prevent_overwrite: false,
            create_directories: true,
            ..Default::default()
        },
    );

    let app = app(config);
    let body = multipart_body(BOUNDARY, "nested.txt", b"Nested content");
    let response = app
        .oneshot(multipart_request("/subdir/nested/", BOUNDARY, body))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NO_CONTENT);
    assert!(public_dir.join("subdir/nested/nested.txt").exists());
}

#[tokio::test]
async fn upload_missing_base_directory_is_created() {
    let temp_dir = TempDir::new().unwrap();
    let public_dir = temp_dir.path().join("public");
    let upload_dir = temp_dir.path().join("uploads_missing");

    fs::create_dir_all(&public_dir).unwrap();

    let mut config = upload_config(
        &public_dir,
        soop3::config::UploadConfig {
            prepend_timestamp: false,
            prevent_overwrite: false,
            create_directories: true,
            ..Default::default()
        },
    );
    config.server.upload_dir = Some(upload_dir.clone());

    let app = app(config);
    let body = multipart_body(BOUNDARY, "upload.txt", b"upload content");
    let response = app
        .oneshot(multipart_request("/", BOUNDARY, body))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NO_CONTENT);
    assert!(upload_dir.exists());
    assert!(upload_dir.join("upload.txt").exists());
}

#[tokio::test]
async fn upload_path_traversal_is_rejected() {
    let temp_dir = TempDir::new().unwrap();
    let public_dir = temp_dir.path();

    let config = upload_config(
        public_dir,
        soop3::config::UploadConfig {
            prepend_timestamp: false,
            prevent_overwrite: false,
            create_directories: false,
            ..Default::default()
        },
    );

    let app = app(config);
    let body = multipart_body(BOUNDARY, "escape.txt", b"malicious content");

    for path in [
        "/../escape.txt",
        "/../../etc/passwd",
        "/subdir/../../../escape.txt",
        "/%2e%2e/escape.txt",
    ] {
        let response = app
            .clone()
            .oneshot(multipart_request(path, BOUNDARY, body.clone()))
            .await
            .unwrap();
        assert_ne!(response.status(), StatusCode::NO_CONTENT);
    }
}

#[cfg(unix)]
#[tokio::test]
async fn upload_symlink_escape_is_rejected() {
    use std::os::unix::fs::symlink;

    let temp_dir = TempDir::new().unwrap();
    let public_dir = temp_dir.path().join("public");
    let upload_dir = temp_dir.path().join("uploads");
    let outside_dir = temp_dir.path().join("outside");

    fs::create_dir_all(&public_dir).unwrap();
    fs::create_dir_all(&upload_dir).unwrap();
    fs::create_dir_all(&outside_dir).unwrap();

    let link_path = upload_dir.join("link");
    symlink(&outside_dir, &link_path).unwrap();

    let mut config = upload_config(
        &public_dir,
        soop3::config::UploadConfig {
            prepend_timestamp: false,
            prevent_overwrite: false,
            ..Default::default()
        },
    );
    config.server.upload_dir = Some(upload_dir);

    let app = app(config);
    let body = multipart_body(BOUNDARY, "escape.txt", b"escape content");
    let response = app
        .oneshot(multipart_request("/link/", BOUNDARY, body))
        .await
        .unwrap();

    assert_ne!(response.status(), StatusCode::NO_CONTENT);
    assert!(!outside_dir.join("escape.txt").exists());
}

#[cfg(unix)]
#[tokio::test]
async fn upload_permission_denied_returns_forbidden() {
    use std::os::unix::fs::PermissionsExt;

    let temp_dir = TempDir::new().unwrap();
    let public_dir = temp_dir.path().join("public");
    let upload_dir = temp_dir.path().join("uploads_read_only");

    fs::create_dir_all(&public_dir).unwrap();
    fs::create_dir_all(&upload_dir).unwrap();

    let mut perms = fs::metadata(&upload_dir).unwrap().permissions();
    perms.set_mode(0o500);
    fs::set_permissions(&upload_dir, perms).unwrap();

    let mut config = upload_config(
        &public_dir,
        soop3::config::UploadConfig {
            prepend_timestamp: false,
            prevent_overwrite: false,
            ..Default::default()
        },
    );
    config.server.upload_dir = Some(upload_dir.clone());

    let app = app(config);
    let body = multipart_body(BOUNDARY, "upload.txt", b"content");
    let response = app
        .oneshot(multipart_request("/", BOUNDARY, body))
        .await
        .unwrap();

    let mut perms = fs::metadata(&upload_dir).unwrap().permissions();
    perms.set_mode(0o700);
    fs::set_permissions(&upload_dir, perms).unwrap();

    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    assert!(!upload_dir.join("upload.txt").exists());
}

#[tokio::test]
async fn upload_rejects_long_filenames() {
    let temp_dir = TempDir::new().unwrap();
    let public_dir = temp_dir.path();

    let config = upload_config(
        public_dir,
        soop3::config::UploadConfig {
            prepend_timestamp: false,
            prevent_overwrite: false,
            ..Default::default()
        },
    );

    let app = app(config);
    let long_name = format!("{}.txt", "a".repeat(300));
    let body = multipart_body(BOUNDARY, &long_name, b"content");
    let response = app
        .oneshot(multipart_request("/", BOUNDARY, body))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}
