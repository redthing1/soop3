// integration tests for soop3 server functionality

use axum::body::Body;
use axum::http::{header, Method, Request, StatusCode};
use base64::Engine;
use soop3::{
    config::{AppConfig, SecurityConfig, SecurityPolicy, ServerConfig, UploadConfig},
    server::create_test_app,
};
use std::fs;
use std::path::PathBuf;
use tempfile::TempDir;
use tower::ServiceExt;

#[tokio::test]
async fn test_file_serving() {
    let temp_dir = TempDir::new().unwrap();
    let public_dir = temp_dir.path();

    // create test files
    fs::write(public_dir.join("test.txt"), "Hello, World!").unwrap();
    fs::create_dir(public_dir.join("subdir")).unwrap();
    fs::write(public_dir.join("subdir/nested.txt"), "Nested content").unwrap();

    let config = AppConfig {
        server: ServerConfig {
            public_dir: public_dir.to_path_buf(),
            host: "127.0.0.1".to_string(),
            port: 8000,
            enable_upload: false,
            upload_dir: None,
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
    assert!(body_str.contains("Index of"));
    assert!(body_str.contains("test.txt"));
    assert!(body_str.contains("subdir"));

    // test file download
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
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    assert_eq!(body, "Hello, World!");

    // test nested file download
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/subdir/nested.txt")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    assert_eq!(body, "Nested content");

    // test 404 for non-existent file
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/nonexistent.txt")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_path_traversal_security() {
    let temp_dir = TempDir::new().unwrap();
    let public_dir = temp_dir.path();

    // create test file in public dir
    fs::write(public_dir.join("public.txt"), "public content").unwrap();

    // create file outside public dir
    let outside_file = temp_dir.path().parent().unwrap().join("secret.txt");
    fs::write(&outside_file, "secret content").unwrap();

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

    // test that legitimate file works
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/public.txt")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    // test various path traversal attempts
    let traversal_attempts = vec![
        "/../secret.txt",
        "/../../secret.txt",
        "/../../../etc/passwd",
        "/..%2fsecret.txt",
        "/%2e%2e/secret.txt",
    ];

    for attempt in traversal_attempts {
        let response = app
            .clone()
            .oneshot(Request::builder().uri(attempt).body(Body::empty()).unwrap())
            .await
            .unwrap();

        // should either be 404 (file not found) or 400 (bad request)
        // but definitely not 200 (success)
        assert_ne!(
            response.status(),
            StatusCode::OK,
            "Path traversal attempt succeeded: {}",
            attempt
        );
    }
}

#[tokio::test]
async fn test_authentication() {
    let temp_dir = TempDir::new().unwrap();
    let public_dir = temp_dir.path();

    fs::write(public_dir.join("test.txt"), "content").unwrap();

    let config = AppConfig {
        server: ServerConfig {
            public_dir: public_dir.to_path_buf(),
            ..Default::default()
        },
        security: SecurityConfig {
            username: Some("admin".to_string()),
            password: Some("secret".to_string()),
            policy: SecurityPolicy::AuthenticateAll,
        },
        ..Default::default()
    };

    let app = create_test_app(config);

    // test that unauthenticated request fails
    let response = app
        .clone()
        .oneshot(Request::builder().uri("/").body(Body::empty()).unwrap())
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);

    // test that authenticated request succeeds
    let auth_header = base64::prelude::BASE64_STANDARD.encode("admin:secret");
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/")
                .header(header::AUTHORIZATION, format!("Basic {}", auth_header))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    // test that wrong credentials fail
    let wrong_auth = base64::prelude::BASE64_STANDARD.encode("admin:wrong");
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/")
                .header(header::AUTHORIZATION, format!("Basic {}", wrong_auth))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn test_upload_functionality() {
    let temp_dir = TempDir::new().unwrap();
    let public_dir = temp_dir.path();

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

    let app = create_test_app(config);

    // create multipart form data for file upload
    let boundary = "----WebKitFormBoundary7MA4YWxkTrZu0gW";
    let body = format!(
        "--{}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"upload.txt\"\r\nContent-Type: text/plain\r\n\r\nUploaded content\r\n--{}--\r\n",
        boundary, boundary
    );

    let response = app
        .clone()
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

    assert_eq!(response.status(), StatusCode::NO_CONTENT);

    // verify file was created
    let uploaded_file = public_dir.join("upload.txt");
    assert!(uploaded_file.exists());
    let content = fs::read_to_string(&uploaded_file).unwrap();
    assert_eq!(content, "Uploaded content");
}

#[tokio::test]
async fn test_security_headers() {
    let temp_dir = TempDir::new().unwrap();
    let public_dir = temp_dir.path();

    fs::write(public_dir.join("test.txt"), "content").unwrap();

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

    let response = app
        .oneshot(
            Request::builder()
                .uri("/test.txt")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    // check that security headers are present
    let headers = response.headers();
    assert!(headers.contains_key("X-Frame-Options"));
    assert!(headers.contains_key("X-Content-Type-Options"));
    assert!(headers.contains_key("X-XSS-Protection"));
    assert!(headers.contains_key("Content-Security-Policy"));
    assert!(headers.contains_key("Referrer-Policy"));

    // verify specific header values
    assert_eq!(headers.get("X-Frame-Options").unwrap(), "DENY");
    assert_eq!(headers.get("X-Content-Type-Options").unwrap(), "nosniff");
    assert_eq!(headers.get("X-XSS-Protection").unwrap(), "1; mode=block");
}

#[tokio::test]
async fn test_static_assets() {
    let temp_dir = TempDir::new().unwrap();
    let public_dir = temp_dir.path();

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

    // test CSS asset
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/__soop_static/style.css")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response.headers().get(header::CONTENT_TYPE).unwrap(),
        "text/css"
    );

    // test favicon
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/__soop_static/favicon.ico")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response.headers().get(header::CONTENT_TYPE).unwrap(),
        "image/x-icon"
    );

    // test 404 for non-existent static asset
    let response = app
        .oneshot(
            Request::builder()
                .uri("/__soop_static/nonexistent.css")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_selective_authentication_policies() {
    let temp_dir = TempDir::new().unwrap();
    let public_dir = temp_dir.path();

    fs::write(public_dir.join("test.txt"), "content").unwrap();

    // test AuthenticateUpload policy
    let config = AppConfig {
        server: ServerConfig {
            public_dir: public_dir.to_path_buf(),
            enable_upload: true,
            ..Default::default()
        },
        security: SecurityConfig {
            username: Some("admin".to_string()),
            password: Some("secret".to_string()),
            policy: SecurityPolicy::AuthenticateUpload,
        },
        ..Default::default()
    };

    let app = create_test_app(config);

    // GET requests should work without authentication
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

    // POST requests should require authentication
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/")
                .header(header::CONTENT_TYPE, "multipart/form-data; boundary=test")
                .body(Body::from("--test\r\nContent-Disposition: form-data; name=\"file\"; filename=\"test.txt\"\r\n\r\ndata\r\n--test--"))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);

    // POST with authentication should work
    let auth_header = base64::prelude::BASE64_STANDARD.encode("admin:secret");
    let response = app
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/")
                .header(header::AUTHORIZATION, format!("Basic {}", auth_header))
                .header(header::CONTENT_TYPE, "multipart/form-data; boundary=test")
                .body(Body::from("--test\r\nContent-Disposition: form-data; name=\"file\"; filename=\"test2.txt\"\r\n\r\ndata\r\n--test--"))
                .unwrap(),
        )
        .await
        .unwrap();

    // should succeed (upload returns 204 No Content)
    assert_eq!(response.status(), StatusCode::NO_CONTENT);
}

#[tokio::test]
async fn test_ignore_file_functionality() {
    let temp_dir = TempDir::new().unwrap();
    let public_dir = temp_dir.path();

    // create test files
    fs::write(public_dir.join("main.rs"), "fn main() {}").unwrap();
    fs::write(public_dir.join("debug.log"), "debug info").unwrap();
    fs::write(public_dir.join("temp_file"), "temporary").unwrap();
    fs::create_dir(public_dir.join("build")).unwrap();
    fs::write(public_dir.join("build/output"), "compiled").unwrap();

    // create ignore file
    fs::write(public_dir.join(".gitignore"), "*.log\ntemp*\nbuild\n").unwrap();

    let config = AppConfig {
        server: ServerConfig {
            public_dir: public_dir.to_path_buf(),
            ..Default::default()
        },
        security: SecurityConfig {
            policy: SecurityPolicy::AuthenticateNone,
            ..Default::default()
        },
        listing: soop3::config::ListingConfig {
            ignore_file: Some(PathBuf::from(".gitignore")),
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

    // should contain main.rs (not ignored)
    assert!(body_str.contains("main.rs"));

    // should NOT contain ignored files
    assert!(!body_str.contains("debug.log"));
    assert!(!body_str.contains("temp_file"));
    assert!(!body_str.contains("build"));
}
