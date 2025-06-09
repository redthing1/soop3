// debug test to understand the 500 error issue

use axum::body::Body;
use axum::http::{Request, StatusCode};
use soop3::{
    config::{AppConfig, SecurityConfig, SecurityPolicy, ServerConfig},
    server::app::create_test_app,
};
use std::fs;
use tempfile::TempDir;
use tower::ServiceExt;

#[tokio::test]
async fn debug_simple_request() {
    let temp_dir = TempDir::new().unwrap();
    let public_dir = temp_dir.path();

    // create a simple test file
    fs::write(public_dir.join("test.txt"), "Hello, World!").unwrap();

    println!("Public dir: {:?}", public_dir);
    println!("Test file exists: {}", public_dir.join("test.txt").exists());

    let config = AppConfig {
        server: ServerConfig {
            public_dir: public_dir.to_path_buf(),
            host: "127.0.0.1".to_string(),
            port: 8000,
            enable_upload: false,
            upload_dir: None,
        },
        security: SecurityConfig {
            username: None,
            password: None,
            policy: SecurityPolicy::AuthenticateNone,
        },
        ..Default::default()
    };

    println!("Config: {:?}", config);

    let app = create_test_app(config);

    // test the request
    let response = app
        .oneshot(
            Request::builder()
                .uri("/test.txt")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    println!("Response status: {}", response.status());
    println!("Response headers: {:?}", response.headers());

    if response.status() == StatusCode::INTERNAL_SERVER_ERROR {
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let body_str = String::from_utf8_lossy(&body);
        println!("Error response body: {}", body_str);
    }

    // don't assert anything, just debug
}
