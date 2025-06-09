// security-focused integration tests

use axum::body::Body;
use axum::http::{header, Method, Request, StatusCode};
use base64::Engine;
use soop3::{
    config::{AppConfig, SecurityConfig, SecurityPolicy, ServerConfig, UploadConfig},
    server::app::create_test_app,
};
use std::fs;
use tempfile::TempDir;
use tower::ServiceExt;

#[tokio::test]
async fn test_comprehensive_path_traversal_attacks() {
    let temp_dir = TempDir::new().unwrap();
    let public_dir = temp_dir.path();

    // create test file structure
    fs::write(public_dir.join("safe.txt"), "safe content").unwrap();

    // create file outside public directory
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

    // test various path traversal attempts
    let attack_vectors = vec![
        "../secret.txt",
        "../../secret.txt",
        "../../../etc/passwd",
        "/etc/passwd",
        "\\..\\secret.txt",
        "%2e%2e/secret.txt",
        "..%2fsecret.txt",
        "%2e%2e%2fsecret.txt",
        "..%252fsecret.txt", // double encoding
        ".%2e/secret.txt",
        "..%5csecret.txt", // backslash encoded
        "..%5c..%5csecret.txt",
        "safe.txt/../secret.txt",
        "safe.txt/../../secret.txt",
        "./../secret.txt",
        ".//../secret.txt",
        "sub/../../../secret.txt",
        // null byte attempts
        "..%00/secret.txt",
        "../secret.txt%00",
        // directory traversal with existing paths
        "safe.txt/../secret.txt",
    ];

    for attack in &attack_vectors {
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri(&format!("/{}", attack))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        // should never return 200 OK for traversal attempts
        assert_ne!(
            response.status(),
            StatusCode::OK,
            "Path traversal succeeded for: {}",
            attack
        );

        // should be either 400 (bad request) or 404 (not found)
        assert!(
            response.status() == StatusCode::BAD_REQUEST
                || response.status() == StatusCode::NOT_FOUND,
            "Unexpected status for path traversal attempt {}: {:?}",
            attack,
            response.status()
        );

        // ensure we never serve the secret content
        if response.status() == StatusCode::OK {
            let body = axum::body::to_bytes(response.into_body(), usize::MAX)
                .await
                .unwrap();
            let body_str = String::from_utf8_lossy(&body);
            assert!(
                !body_str.contains("secret content"),
                "Secret content leaked for path: {}",
                attack
            );
        }
    }

    // verify legitimate access still works
    let response = app
        .oneshot(
            Request::builder()
                .uri("/safe.txt")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    assert_eq!(body, "safe content");
}

#[tokio::test]
async fn test_upload_path_traversal_attacks() {
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
            create_directories: false, // disable directory creation for security
            ..Default::default()
        },
        ..Default::default()
    };

    let app = create_test_app(config);

    let boundary = "----WebKitFormBoundary7MA4YWxkTrZu0gW";

    // test uploading to paths that attempt traversal
    let malicious_paths = vec![
        "../escape.txt",
        "../../etc/passwd",
        "/etc/passwd",
        "subdir/../../../escape.txt",
        "%2e%2e/escape.txt",
    ];

    for malicious_path in &malicious_paths {
        let body = format!(
            "--{}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"test.txt\"\r\nContent-Type: text/plain\r\n\r\nmalicious content\r\n--{}--\r\n",
            boundary, boundary
        );

        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri(&format!("/{}", malicious_path))
                    .header(
                        header::CONTENT_TYPE,
                        format!("multipart/form-data; boundary={}", boundary),
                    )
                    .body(Body::from(body))
                    .unwrap(),
            )
            .await
            .unwrap();

        // should reject malicious upload paths
        assert_ne!(
            response.status(),
            StatusCode::NO_CONTENT,
            "Upload traversal succeeded for: {}",
            malicious_path
        );
        assert!(
            response.status() == StatusCode::BAD_REQUEST
                || response.status() == StatusCode::NOT_FOUND,
            "Unexpected status for upload traversal attempt {}: {:?}",
            malicious_path,
            response.status()
        );
    }

    // verify legitimate uploads still work
    let body = format!(
        "--{}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"legitimate.txt\"\r\nContent-Type: text/plain\r\n\r\nlegitimate content\r\n--{}--\r\n",
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

    assert_eq!(response.status(), StatusCode::NO_CONTENT);
    assert!(public_dir.join("legitimate.txt").exists());
}

#[tokio::test]
async fn test_authentication_bypass_attempts() {
    let temp_dir = TempDir::new().unwrap();
    let public_dir = temp_dir.path();

    fs::write(public_dir.join("protected.txt"), "protected content").unwrap();

    let config = AppConfig {
        server: ServerConfig {
            public_dir: public_dir.to_path_buf(),
            enable_upload: true,
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

    // test various authentication bypass attempts
    let bypass_attempts = vec![
        // malformed auth headers
        "Basic",
        "Basic ",
        "Basic invalid-base64",
        "Basic dGVzdA==",     // "test" - missing colon
        "Bearer token",       // wrong auth type
        "basic dGVzdDp0ZXN0", // wrong case
        // injection attempts
        "Basic YWRtaW46c2VjcmV0O2xzIC1sYQ==", // admin:secret;ls -la
    ];

    for auth_header in &bypass_attempts {
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/protected.txt")
                    .header(header::AUTHORIZATION, *auth_header)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(
            response.status(),
            StatusCode::UNAUTHORIZED,
            "Auth bypass succeeded with header: {}",
            auth_header
        );
    }

    // test correct authentication still works
    let valid_auth = base64::prelude::BASE64_STANDARD.encode("admin:secret");
    let response = app
        .oneshot(
            Request::builder()
                .uri("/protected.txt")
                .header(header::AUTHORIZATION, format!("Basic {}", valid_auth))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_timing_attack_resistance() {
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
            password: Some("verylongpasswordthatmightrevealtiminginformation".to_string()),
            policy: SecurityPolicy::AuthenticateAll,
        },
        ..Default::default()
    };

    let app = create_test_app(config);

    // this test doesn't measure actual timing (would be flaky)
    // but verifies that different wrong credentials all fail consistently
    let wrong_credentials = vec![
        "wrong:password",
        "admin:wrong",
        "admin:short",
        "admin:verylongwrongpasswordthatmightrevealtiminginformation",
        "wronguser:verylongpasswordthatmightrevealtiminginformation",
        "a:b",
        "admin:",                                            // empty password
        ":verylongpasswordthatmightrevealtiminginformation", // empty username
    ];

    for creds in &wrong_credentials {
        let auth_header = base64::prelude::BASE64_STANDARD.encode(creds);
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/test.txt")
                    .header(header::AUTHORIZATION, format!("Basic {}", auth_header))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        // all wrong credentials should fail with same status
        assert_eq!(
            response.status(),
            StatusCode::UNAUTHORIZED,
            "Inconsistent response for credentials: {}",
            creds
        );
    }
}

#[tokio::test]
async fn test_malicious_upload_content() {
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
            max_request_size: 10240,
            ..Default::default()
        },
        ..Default::default()
    };

    let app = create_test_app(config);

    let boundary = "----WebKitFormBoundary7MA4YWxkTrZu0gW";

    // test various potentially malicious content types
    let malicious_contents = vec![
        // binary content that might cause issues
        (b"\x00\x01\x02\xFF".to_vec(), "binary.bin"),
        // very long filename
        ("x".repeat(1000).as_bytes().to_vec(), "long.txt"),
        // script content (should still be accepted but served safely)
        (b"<script>alert('xss')</script>".to_vec(), "script.html"),
        // null bytes in content
        (b"content\x00with\x00nulls".to_vec(), "nulls.txt"),
    ];

    for (content, filename) in &malicious_contents {
        let body_content = [
            format!("--{}\r\n", boundary).as_bytes(),
            format!(
                "Content-Disposition: form-data; name=\"file\"; filename=\"{}\"\r\n",
                filename
            )
            .as_bytes(),
            b"Content-Type: application/octet-stream\r\n\r\n",
            content.as_slice(),
            format!("\r\n--{}--\r\n", boundary).as_bytes(),
        ]
        .concat();

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
                    .body(Body::from(body_content))
                    .unwrap(),
            )
            .await
            .unwrap();

        // uploads should succeed (content filtering is not soop's responsibility)
        // but the server should handle them safely
        if filename.len() <= 255 {
            // reasonable filename length
            assert!(
                response.status() == StatusCode::NO_CONTENT
                    || response.status() == StatusCode::BAD_REQUEST,
                "Unexpected status for file {}: {:?}",
                filename,
                response.status()
            );
        } else {
            // very long filenames should be rejected
            assert_eq!(
                response.status(),
                StatusCode::BAD_REQUEST,
                "Should reject very long filename"
            );
        }
    }
}

#[tokio::test]
async fn test_security_headers_comprehensive() {
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

    let headers = response.headers();

    // verify all security headers are present and have secure values
    assert_eq!(headers.get("X-Frame-Options").unwrap(), "DENY");
    assert_eq!(headers.get("X-Content-Type-Options").unwrap(), "nosniff");
    assert_eq!(headers.get("X-XSS-Protection").unwrap(), "1; mode=block");
    assert_eq!(
        headers.get("Referrer-Policy").unwrap(),
        "strict-origin-when-cross-origin"
    );

    let csp = headers
        .get("Content-Security-Policy")
        .unwrap()
        .to_str()
        .unwrap();
    assert!(csp.contains("default-src 'self'"));
    assert!(csp.contains("object-src 'none'"));

    // ensure no sensitive headers are leaked
    assert!(headers.get("Server").is_none()); // don't reveal server info
    assert!(headers.get("X-Powered-By").is_none()); // don't reveal technology
}
