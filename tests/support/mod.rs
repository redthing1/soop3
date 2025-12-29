// shared test helpers
#![allow(dead_code)] // helpers are shared across multiple integration test crates

use axum::body::Body;
use axum::http::{Method, Request, header};
use axum::response::Response;
use base64::Engine;
use soop3::{
    config::{AppConfig, SecurityConfig, SecurityPolicy, ServerConfig, UploadConfig},
    server::app::create_test_app,
};
use std::path::Path;

pub const BOUNDARY: &str = "----soop3-boundary";

pub fn base_config(public_dir: &Path) -> AppConfig {
    AppConfig {
        server: ServerConfig {
            public_dir: public_dir.to_path_buf(),
            ..Default::default()
        },
        security: SecurityConfig {
            policy: SecurityPolicy::AuthenticateNone,
            ..Default::default()
        },
        ..Default::default()
    }
}

pub fn upload_config(public_dir: &Path, upload: UploadConfig) -> AppConfig {
    let mut config = base_config(public_dir);
    config.server.enable_upload = true;
    config.upload = upload;
    config
}

pub fn app(config: AppConfig) -> axum::Router {
    create_test_app(config)
}

pub fn get(uri: &str) -> Request<Body> {
    Request::builder()
        .method(Method::GET)
        .uri(uri)
        .body(Body::empty())
        .unwrap()
}

pub fn get_with_range(uri: &str, range: &str) -> Request<Body> {
    Request::builder()
        .method(Method::GET)
        .uri(uri)
        .header(header::RANGE, range)
        .body(Body::empty())
        .unwrap()
}

pub fn head(uri: &str) -> Request<Body> {
    Request::builder()
        .method(Method::HEAD)
        .uri(uri)
        .body(Body::empty())
        .unwrap()
}

pub fn head_with_range(uri: &str, range: &str) -> Request<Body> {
    Request::builder()
        .method(Method::HEAD)
        .uri(uri)
        .header(header::RANGE, range)
        .body(Body::empty())
        .unwrap()
}

pub fn multipart_body(boundary: &str, filename: &str, content: &[u8]) -> Vec<u8> {
    [
        format!("--{boundary}\r\n").as_bytes(),
        format!("Content-Disposition: form-data; name=\"file\"; filename=\"{filename}\"\r\n")
            .as_bytes(),
        b"Content-Type: text/plain\r\n\r\n",
        content,
        format!("\r\n--{boundary}--\r\n").as_bytes(),
    ]
    .concat()
}

pub fn multipart_request(uri: &str, boundary: &str, body: Vec<u8>) -> Request<Body> {
    Request::builder()
        .method(Method::POST)
        .uri(uri)
        .header(
            header::CONTENT_TYPE,
            format!("multipart/form-data; boundary={boundary}"),
        )
        .body(Body::from(body))
        .unwrap()
}

pub fn auth_header(username: &str, password: &str) -> String {
    base64::prelude::BASE64_STANDARD.encode(format!("{username}:{password}"))
}

pub async fn body_string(response: Response) -> String {
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    String::from_utf8(bytes.to_vec()).unwrap()
}
