// file serving and directory listing behavior

mod support;

use axum::http::StatusCode;
use support::{app, base_config, body_string, get};
use tempfile::TempDir;
use tower::ServiceExt;

use std::fs;
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

#[tokio::test]
async fn serves_files_and_directory_listings() {
    let temp_dir = TempDir::new().unwrap();
    let public_dir = temp_dir.path();

    fs::write(public_dir.join("test.txt"), "Hello, World!").unwrap();
    fs::create_dir(public_dir.join("subdir")).unwrap();
    fs::write(public_dir.join("subdir/nested.txt"), "Nested content").unwrap();

    let app = app(base_config(public_dir));

    let response = app.clone().oneshot(get("/")).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = body_string(response).await;
    assert!(body.contains("Index of"));
    assert!(body.contains("test.txt"));
    assert!(body.contains("subdir"));

    let response = app.clone().oneshot(get("/test.txt")).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = body_string(response).await;
    assert_eq!(body, "Hello, World!");

    let response = app
        .clone()
        .oneshot(get("/subdir/nested.txt"))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = body_string(response).await;
    assert_eq!(body, "Nested content");

    let response = app.clone().oneshot(get("/nonexistent.txt")).await.unwrap();
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn serves_directory_index_files_with_precedence() {
    let temp_dir = TempDir::new().unwrap();
    let public_dir = temp_dir.path();

    fs::create_dir(public_dir.join("with_index")).unwrap();
    fs::write(
        public_dir.join("with_index/index.html"),
        "<h1>Directory Index</h1>",
    )
    .unwrap();

    fs::create_dir(public_dir.join("with_index_htm")).unwrap();
    fs::write(
        public_dir.join("with_index_htm/index.htm"),
        "<h1>HTM Index</h1>",
    )
    .unwrap();

    fs::create_dir(public_dir.join("both_indexes")).unwrap();
    fs::write(
        public_dir.join("both_indexes/index.html"),
        "<h1>HTML Index</h1>",
    )
    .unwrap();
    fs::write(
        public_dir.join("both_indexes/index.htm"),
        "<h1>HTM Index</h1>",
    )
    .unwrap();

    let app = app(base_config(public_dir));

    let response = app.clone().oneshot(get("/with_index/")).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = body_string(response).await;
    assert!(body.contains("Directory Index"));

    let response = app.clone().oneshot(get("/with_index_htm/")).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = body_string(response).await;
    assert!(body.contains("HTM Index"));

    let response = app.clone().oneshot(get("/both_indexes/")).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = body_string(response).await;
    assert!(body.contains("HTML Index"));
    assert!(!body.contains("HTM Index"));
}

#[tokio::test]
async fn renders_directory_links_and_url_encoding() {
    let temp_dir = TempDir::new().unwrap();
    let public_dir = temp_dir.path();

    fs::create_dir_all(public_dir.join("level1/level2")).unwrap();
    fs::write(public_dir.join("level1/level2/file.txt"), "content").unwrap();
    fs::write(public_dir.join("level1/file1.txt"), "content1").unwrap();
    fs::write(public_dir.join("file with spaces.txt"), "content").unwrap();
    fs::write(public_dir.join("file#hash.txt"), "content").unwrap();

    let app = app(base_config(public_dir));

    let response = app.clone().oneshot(get("/")).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = body_string(response).await;
    assert!(body.contains("href=\"/level1/\""));
    assert!(body.contains("href=\"/file%20with%20spaces.txt\""));
    assert!(body.contains(">file with spaces.txt<"));
    assert!(body.contains("href=\"/file%23hash.txt\""));
    assert!(body.contains(">file#hash.txt<"));

    let response = app.clone().oneshot(get("/level1/")).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = body_string(response).await;
    assert!(body.contains("href=\"/level1/level2/\""));
    assert!(body.contains("href=\"/level1/file1.txt\""));
    assert!(!body.contains("href=\"/level1/level1/"));

    let response = app.oneshot(get("/level1/level2/")).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = body_string(response).await;
    assert!(body.contains("href=\"/level1/level2/file.txt\""));
    assert!(!body.contains("href=\"/level1/level2/level1/level2/"));
}

#[tokio::test]
async fn applies_ignore_file_in_nested_directories() {
    let temp_dir = TempDir::new().unwrap();
    let public_dir = temp_dir.path();

    fs::create_dir_all(public_dir.join("nested")).unwrap();
    fs::write(public_dir.join("nested/hidden.log"), "hidden").unwrap();
    fs::write(public_dir.join("nested/visible.txt"), "visible").unwrap();
    fs::write(public_dir.join(".gitignore"), "nested/*.log\n").unwrap();

    let mut config = base_config(public_dir);
    config.listing.ignore_file = Some(".gitignore".into());

    let app = app(config);
    let response = app.oneshot(get("/nested/")).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = body_string(response).await;
    assert!(body.contains("visible.txt"));
    assert!(!body.contains("hidden.log"));
}

#[tokio::test]
async fn rejects_path_traversal_attempts() {
    let temp_dir = TempDir::new().unwrap();
    let public_dir = temp_dir.path();

    fs::write(public_dir.join("public.txt"), "public content").unwrap();
    let outside_file = temp_dir.path().parent().unwrap().join("secret.txt");
    fs::write(&outside_file, "secret content").unwrap();

    let app = app(base_config(public_dir));

    let response = app.clone().oneshot(get("/public.txt")).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    for attempt in [
        "/../secret.txt",
        "/../../secret.txt",
        "/../../../etc/passwd",
        "/..%2fsecret.txt",
        "/%2e%2e/secret.txt",
    ] {
        let response = app.clone().oneshot(get(attempt)).await.unwrap();
        assert_ne!(
            response.status(),
            StatusCode::OK,
            "Traversal attempt succeeded: {attempt}"
        );
    }
}

#[tokio::test]
async fn rejects_encoded_slash_in_path() {
    let temp_dir = TempDir::new().unwrap();
    let public_dir = temp_dir.path();

    fs::create_dir(public_dir.join("dir")).unwrap();
    fs::write(public_dir.join("dir/file.txt"), "content").unwrap();

    let app = app(base_config(public_dir));
    let response = app.clone().oneshot(get("/dir%2Ffile.txt")).await.unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[cfg(unix)]
#[tokio::test]
async fn permission_denied_file_returns_forbidden() {
    let temp_dir = TempDir::new().unwrap();
    let public_dir = temp_dir.path();
    let file_path = public_dir.join("secret.txt");

    fs::write(&file_path, "secret").unwrap();
    fs::set_permissions(&file_path, fs::Permissions::from_mode(0o000)).unwrap();

    let app = app(base_config(public_dir));
    let response = app.clone().oneshot(get("/secret.txt")).await.unwrap();

    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    fs::set_permissions(&file_path, fs::Permissions::from_mode(0o600)).unwrap();
}

#[cfg(unix)]
#[tokio::test]
async fn permission_denied_directory_returns_forbidden() {
    let temp_dir = TempDir::new().unwrap();
    let public_dir = temp_dir.path();
    let dir_path = public_dir.join("locked");

    fs::create_dir(&dir_path).unwrap();
    fs::set_permissions(&dir_path, fs::Permissions::from_mode(0o000)).unwrap();

    let app = app(base_config(public_dir));
    let response = app.clone().oneshot(get("/locked/")).await.unwrap();

    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    fs::set_permissions(&dir_path, fs::Permissions::from_mode(0o700)).unwrap();
}

#[tokio::test]
async fn serves_unicode_and_hidden_files() {
    let temp_dir = TempDir::new().unwrap();
    let public_dir = temp_dir.path();

    fs::write(public_dir.join("🌍.txt"), "unicode filename").unwrap();
    fs::write(public_dir.join(".hidden"), "hidden file").unwrap();
    fs::write(public_dir.join("CAPS.TXT"), "uppercase ext").unwrap();

    let app = app(base_config(public_dir));

    let response = app.clone().oneshot(get("/%F0%9F%8C%8D.txt")).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = body_string(response).await;
    assert_eq!(body, "unicode filename");

    let response = app.clone().oneshot(get("/.hidden")).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = body_string(response).await;
    assert_eq!(body, "hidden file");

    let response = app.clone().oneshot(get("/CAPS.TXT")).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = body_string(response).await;
    assert_eq!(body, "uppercase ext");
}

#[tokio::test]
async fn serves_percent_encoded_filenames_without_double_decode() {
    let temp_dir = TempDir::new().unwrap();
    let public_dir = temp_dir.path();

    fs::write(public_dir.join("%2F.txt"), "encoded slash").unwrap();

    let app = app(base_config(public_dir));
    let response = app.clone().oneshot(get("/%252F.txt")).await.unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = body_string(response).await;
    assert_eq!(body, "encoded slash");
}
