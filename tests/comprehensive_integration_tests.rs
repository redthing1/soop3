// comprehensive integration tests for soop3 server functionality
// covers all major features, edge cases, and error conditions

use axum::body::Body;
use axum::http::{Method, Request, StatusCode, header};
use base64::Engine;
use soop3::{
    config::{AppConfig, SecurityConfig, SecurityPolicy, ServerConfig, UploadConfig},
    server::app::create_test_app,
};
use std::fs;
use tempfile::TempDir;
use tower::ServiceExt;

// === FILE SERVING COMPREHENSIVE TESTS ===

#[tokio::test]
async fn test_comprehensive_file_serving() {
    let temp_dir = TempDir::new().unwrap();
    let public_dir = temp_dir.path();

    // create comprehensive test file structure
    fs::write(
        public_dir.join("index.html"),
        "<html><body>Index</body></html>",
    )
    .unwrap();
    fs::write(public_dir.join("plain.txt"), "Plain text content").unwrap();
    fs::write(public_dir.join("binary.pdf"), b"\x25\x50\x44\x46").unwrap(); // PDF header
    fs::write(public_dir.join("empty.txt"), "").unwrap();
    fs::write(public_dir.join("unicode.txt"), "Hello 世界 🌍").unwrap();

    // special file names
    fs::write(public_dir.join("file with spaces.txt"), "spaced content").unwrap();
    fs::write(public_dir.join("file-with-dashes.txt"), "dashed content").unwrap();
    fs::write(public_dir.join("file.with.dots.txt"), "dotted content").unwrap();

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

    // test various file types
    let test_cases = vec![
        (
            "/plain.txt",
            StatusCode::OK,
            "text/plain",
            "Plain text content",
        ),
        ("/empty.txt", StatusCode::OK, "text/plain", ""),
        (
            "/unicode.txt",
            StatusCode::OK,
            "text/plain",
            "Hello 世界 🌍",
        ),
        ("/binary.pdf", StatusCode::OK, "application/pdf", "%PDF"), // partial match
    ];

    for (path, expected_status, expected_content_type, expected_content) in test_cases {
        let response = app
            .clone()
            .oneshot(Request::builder().uri(path).body(Body::empty()).unwrap())
            .await
            .unwrap();

        assert_eq!(
            response.status(),
            expected_status,
            "Failed for path: {}",
            path
        );
        assert!(
            response
                .headers()
                .get(header::CONTENT_TYPE)
                .unwrap()
                .to_str()
                .unwrap()
                .contains(expected_content_type),
            "Wrong content type for path: {}",
            path
        );

        if !expected_content.is_empty() {
            let body = axum::body::to_bytes(response.into_body(), usize::MAX)
                .await
                .unwrap();
            let body_str = String::from_utf8_lossy(&body);
            assert!(
                body_str.contains(expected_content),
                "Wrong content for path: {}",
                path
            );
        }
    }

    // test URL encoded file names
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/file%20with%20spaces.txt")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    assert_eq!(body, "spaced content");
}

#[tokio::test]
async fn test_directory_index_files() {
    let temp_dir = TempDir::new().unwrap();
    let public_dir = temp_dir.path();

    // create directory structure with index files
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

    fs::create_dir(public_dir.join("no_index")).unwrap();
    fs::write(public_dir.join("no_index/other.html"), "<h1>Other</h1>").unwrap();

    // test precedence: index.html over index.htm
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

    // test directory with index.html
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/with_index/")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    assert!(String::from_utf8_lossy(&body).contains("Directory Index"));

    // test directory with index.htm
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/with_index_htm/")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    assert!(String::from_utf8_lossy(&body).contains("HTM Index"));

    // test directory without index file (should show listing)
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/no_index/")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let body_str = String::from_utf8_lossy(&body);
    assert!(body_str.contains("Index of"));
    assert!(body_str.contains("other.html"));

    // test precedence: index.html should win over index.htm
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/both_indexes/")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    assert!(String::from_utf8_lossy(&body).contains("HTML Index"));
    assert!(!String::from_utf8_lossy(&body).contains("HTM Index"));
}

// === UPLOAD COMPREHENSIVE TESTS ===

#[tokio::test]
async fn test_comprehensive_upload_functionality() {
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
            prevent_overwrite: true,
            max_request_size: 1024,
            create_directories: true,
        },
        ..Default::default()
    };

    let app = create_test_app(config);

    // test successful upload
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
    assert!(public_dir.join("upload.txt").exists());
    assert_eq!(
        fs::read_to_string(public_dir.join("upload.txt")).unwrap(),
        "Uploaded content"
    );

    // test upload to subdirectory (with create_directories=true)
    let body = format!(
        "--{}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"upload2.txt\"\r\nContent-Type: text/plain\r\n\r\nSubdir content\r\n--{}--\r\n",
        boundary, boundary
    );

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/subdir/nested/")
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
    assert!(public_dir.join("subdir/nested/upload2.txt").exists());

    // test upload with same filename (should fail due to prevent_overwrite=true)
    let body = format!(
        "--{}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"upload.txt\"\r\nContent-Type: text/plain\r\n\r\nOverwrite attempt\r\n--{}--\r\n",
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

    assert_eq!(response.status(), StatusCode::CONFLICT);
    // file should not be overwritten
    assert_eq!(
        fs::read_to_string(public_dir.join("upload.txt")).unwrap(),
        "Uploaded content"
    );

    // test upload too large (max_request_size=1024)
    let large_content = "x".repeat(2000);
    let body = format!(
        "--{}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"large.txt\"\r\nContent-Type: text/plain\r\n\r\n{}\r\n--{}--\r\n",
        boundary, large_content, boundary
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

    assert_eq!(response.status(), StatusCode::PAYLOAD_TOO_LARGE);
    assert!(!public_dir.join("large.txt").exists());
}

#[tokio::test]
async fn test_upload_disabled() {
    let temp_dir = TempDir::new().unwrap();
    let public_dir = temp_dir.path();

    let config = AppConfig {
        server: ServerConfig {
            public_dir: public_dir.to_path_buf(),
            enable_upload: false, // uploads disabled
            ..Default::default()
        },
        security: SecurityConfig {
            policy: SecurityPolicy::AuthenticateNone,
            ..Default::default()
        },
        ..Default::default()
    };

    let app = create_test_app(config);

    let boundary = "----WebKitFormBoundary7MA4YWxkTrZu0gW";
    let body = format!(
        "--{}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"upload.txt\"\r\nContent-Type: text/plain\r\n\r\nContent\r\n--{}--\r\n",
        boundary, boundary
    );

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

    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    assert!(!public_dir.join("upload.txt").exists());
}

// === AUTHENTICATION COMPREHENSIVE TESTS ===

#[tokio::test]
async fn test_comprehensive_authentication() {
    let temp_dir = TempDir::new().unwrap();
    let public_dir = temp_dir.path();

    fs::write(public_dir.join("test.txt"), "content").unwrap();

    // test all authentication policies
    let test_cases = vec![
        (SecurityPolicy::AuthenticateNone, Method::GET, false),
        (SecurityPolicy::AuthenticateNone, Method::POST, false),
        (SecurityPolicy::AuthenticateAll, Method::GET, true),
        (SecurityPolicy::AuthenticateAll, Method::POST, true),
        (SecurityPolicy::AuthenticateUpload, Method::GET, false),
        (SecurityPolicy::AuthenticateUpload, Method::POST, true),
        (SecurityPolicy::AuthenticateDownload, Method::GET, true),
        (SecurityPolicy::AuthenticateDownload, Method::POST, false),
    ];

    for (policy, method, should_require_auth) in test_cases {
        let config = AppConfig {
            server: ServerConfig {
                public_dir: public_dir.to_path_buf(),
                enable_upload: true,
                ..Default::default()
            },
            security: SecurityConfig {
                username: Some("admin".to_string()),
                password: Some("secret".to_string()),
                policy,
            },
            ..Default::default()
        };

        let app = create_test_app(config);

        let request = if method == Method::POST {
            Request::builder()
                .method(Method::POST)
                .uri("/")
                .header(header::CONTENT_TYPE, "multipart/form-data; boundary=test")
                .body(Body::from("--test\r\nContent-Disposition: form-data; name=\"file\"; filename=\"test.txt\"\r\n\r\ndata\r\n--test--"))
                .unwrap()
        } else {
            Request::builder()
                .method(Method::GET)
                .uri("/test.txt")
                .body(Body::empty())
                .unwrap()
        };

        let response = app.clone().oneshot(request).await.unwrap();

        if should_require_auth {
            assert_eq!(
                response.status(),
                StatusCode::UNAUTHORIZED,
                "Policy {:?} with method {:?} should require auth",
                policy,
                method
            );

            // test with correct credentials
            let auth_header = base64::prelude::BASE64_STANDARD.encode("admin:secret");
            let request_with_auth = if method == Method::POST {
                Request::builder()
                    .method(Method::POST)
                    .uri("/")
                    .header(header::AUTHORIZATION, format!("Basic {}", auth_header))
                    .header(header::CONTENT_TYPE, "multipart/form-data; boundary=test")
                    .body(Body::from("--test\r\nContent-Disposition: form-data; name=\"file\"; filename=\"test2.txt\"\r\n\r\ndata\r\n--test--"))
                    .unwrap()
            } else {
                Request::builder()
                    .method(Method::GET)
                    .uri("/test.txt")
                    .header(header::AUTHORIZATION, format!("Basic {}", auth_header))
                    .body(Body::empty())
                    .unwrap()
            };

            let response_with_auth = app.oneshot(request_with_auth).await.unwrap();
            assert_ne!(
                response_with_auth.status(),
                StatusCode::UNAUTHORIZED,
                "Valid auth should work for policy {:?} with method {:?}",
                policy,
                method
            );
        } else {
            assert_ne!(
                response.status(),
                StatusCode::UNAUTHORIZED,
                "Policy {:?} with method {:?} should not require auth",
                policy,
                method
            );
        }
    }
}

// === ERROR HANDLING TESTS ===

#[tokio::test]
async fn test_error_conditions() {
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

    // test 404 for non-existent directory
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/nonexistent/")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::NOT_FOUND);

    // test bad multipart data - should fail because uploads are disabled by default
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/")
                .header(header::CONTENT_TYPE, "multipart/form-data; boundary=test")
                .body(Body::from("invalid multipart data"))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::FORBIDDEN); // uploads disabled, not bad request

    // test missing multipart boundary - should also fail because uploads disabled
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/")
                .header(header::CONTENT_TYPE, "multipart/form-data")
                .body(Body::from("data"))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST); // malformed multipart, fails before upload check
}

// === EDGE CASE TESTS ===

#[tokio::test]
async fn test_edge_cases() {
    let temp_dir = TempDir::new().unwrap();
    let public_dir = temp_dir.path();

    // create files with edge case names
    fs::write(public_dir.join("🌍.txt"), "unicode filename").unwrap();
    fs::write(public_dir.join(".hidden"), "hidden file").unwrap();
    fs::write(public_dir.join("CAPS.TXT"), "uppercase ext").unwrap();

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

    // test unicode filename
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/%F0%9F%8C%8D.txt")
                .body(Body::empty())
                .unwrap(),
        ) // URL encoded 🌍
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    assert_eq!(body, "unicode filename");

    // test hidden file
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/.hidden")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    assert_eq!(body, "hidden file");

    // test case sensitivity
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/CAPS.TXT")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    // test empty path components (should be handled gracefully)
    let response = app
        .clone()
        .oneshot(Request::builder().uri("//").body(Body::empty()).unwrap())
        .await
        .unwrap();
    // should either redirect to / or serve root directory listing
    assert!(
        response.status() == StatusCode::OK || response.status() == StatusCode::MOVED_PERMANENTLY
    );
}
