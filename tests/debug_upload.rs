// debug upload functionality

use axum::body::Body;
use axum::http::{Method, Request, StatusCode, header};
use soop3::{
    config::{AppConfig, SecurityConfig, SecurityPolicy, ServerConfig, UploadConfig},
    server::app::create_test_app,
};
use tempfile::TempDir;
use tower::ServiceExt;

#[tokio::test]
async fn debug_upload_simple() {
    let temp_dir = TempDir::new().unwrap();
    let public_dir = temp_dir.path();

    println!("Public dir: {:?}", public_dir);

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
            prepend_timestamp: false,
            prevent_overwrite: false,
            ..Default::default()
        },
        ..Default::default()
    };

    println!("Config uploads enabled: {}", config.server.enable_upload);

    let app = create_test_app(config);

    // test very simple POST request first
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/")
                .header(header::CONTENT_TYPE, "text/plain")
                .body(Body::from("simple test"))
                .unwrap(),
        )
        .await
        .unwrap();

    println!("Simple POST status: {}", response.status());
    println!("Simple POST headers: {:?}", response.headers());

    if response.status() != StatusCode::OK {
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let body_str = String::from_utf8_lossy(&body);
        println!("Error response body: {}", body_str);
    }

    // now test multipart
    let boundary = "----WebKitFormBoundary7MA4YWxkTrZu0gW";
    let body = format!(
        "--{}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"upload.txt\"\r\nContent-Type: text/plain\r\n\r\nUploaded content\r\n--{}--\r\n",
        boundary, boundary
    );

    println!("Multipart body: {}", body);

    let response = app
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/")
                .header(
                    header::CONTENT_TYPE,
                    format!("multipart/form-data; boundary={}", boundary),
                )
                .body(Body::from(body))
                .unwrap(),
        )
        .await
        .unwrap();

    println!("Multipart POST status: {}", response.status());
    println!("Multipart POST headers: {:?}", response.headers());

    if response.status() != StatusCode::OK
        && response.status() != StatusCode::CREATED
        && response.status() != StatusCode::NO_CONTENT
    {
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let body_str = String::from_utf8_lossy(&body);
        println!("Error response body: {}", body_str);
    }
}
