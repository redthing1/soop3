// test to verify directory links work correctly

use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use soop3::{
    config::{AppConfig, SecurityConfig, SecurityPolicy, ServerConfig},
    server::app::create_test_app,
};
use std::fs;
use tempfile::TempDir;
use tower::ServiceExt;

#[tokio::test]
async fn test_directory_links_are_correct() {
    let temp_dir = TempDir::new().unwrap();
    let public_dir = temp_dir.path();

    // create nested directory structure
    fs::create_dir_all(public_dir.join("level1/level2")).unwrap();
    fs::write(public_dir.join("level1/level2/file.txt"), "content").unwrap();
    fs::write(public_dir.join("level1/file1.txt"), "content1").unwrap();

    let config = AppConfig {
        server: ServerConfig {
            public_dir: public_dir.to_path_buf(),
            ..Default::default()
        },
        security: SecurityConfig {
            policy: SecurityPolicy::AuthenticateNone,
            ..Default::default()
        },
        ..Default::default()
    };

    let app = create_test_app(config);

    // test root directory listing
    let response = app
        .clone()
        .oneshot(Request::builder().uri("/").body(Body::empty()).unwrap())
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let body_str = String::from_utf8(body.to_vec()).unwrap();

    // verify level1 link is correct (should be /level1/)
    assert!(body_str.contains("href=\"/level1/\""));

    // test level1 directory listing
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/level1/")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let body_str = String::from_utf8(body.to_vec()).unwrap();

    // verify links are correct (should be /level1/level2/ and /level1/file1.txt)
    assert!(body_str.contains("href=\"/level1/level2/\""));
    assert!(body_str.contains("href=\"/level1/file1.txt\""));

    // should NOT contain doubled paths like /level1/level1/
    assert!(!body_str.contains("href=\"/level1/level1/"));

    // test level2 directory listing
    let response = app
        .oneshot(
            Request::builder()
                .uri("/level1/level2/")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let body_str = String::from_utf8(body.to_vec()).unwrap();

    // verify file link is correct (should be /level1/level2/file.txt)
    assert!(body_str.contains("href=\"/level1/level2/file.txt\""));

    // should NOT contain tripled paths like /level1/level2/level1/level2/
    assert!(!body_str.contains("href=\"/level1/level2/level1/level2/"));
}
