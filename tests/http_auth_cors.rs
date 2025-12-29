// authentication, CORS, and security headers

mod support;

use axum::http::{Method, StatusCode, header};
use base64::Engine;
use soop3::config::{SecurityConfig, SecurityPolicy, UploadConfig};
use support::{BOUNDARY, app, auth_header, base_config, get, multipart_body, multipart_request};
use tempfile::TempDir;
use tower::ServiceExt;

use std::fs;

#[tokio::test]
async fn basic_authentication_requires_credentials() {
    let temp_dir = TempDir::new().unwrap();
    let public_dir = temp_dir.path();

    fs::write(public_dir.join("test.txt"), "content").unwrap();

    let mut config = base_config(public_dir);
    config.security = SecurityConfig {
        username: Some("admin".to_string()),
        password: Some("secret".to_string()),
        policy: SecurityPolicy::AuthenticateAll,
    };

    let app = app(config);

    let response = app.clone().oneshot(get("/test.txt")).await.unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    assert!(response.headers().contains_key(header::WWW_AUTHENTICATE));

    let auth_value = auth_header("admin", "secret");
    let response = app
        .clone()
        .oneshot(
            axum::http::Request::builder()
                .method(Method::GET)
                .uri("/test.txt")
                .header(header::AUTHORIZATION, format!("Basic {auth_value}"))
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let wrong_auth = auth_header("admin", "wrong");
    let response = app
        .oneshot(
            axum::http::Request::builder()
                .method(Method::GET)
                .uri("/test.txt")
                .header(header::AUTHORIZATION, format!("Basic {wrong_auth}"))
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn authentication_policies_match_methods() {
    let temp_dir = TempDir::new().unwrap();
    let public_dir = temp_dir.path();

    fs::write(public_dir.join("test.txt"), "content").unwrap();

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
        let mut config = base_config(public_dir);
        config.server.enable_upload = true;
        config.security = SecurityConfig {
            username: Some("admin".to_string()),
            password: Some("secret".to_string()),
            policy,
        };
        config.upload = UploadConfig {
            prepend_timestamp: false,
            prevent_overwrite: false,
            ..Default::default()
        };

        let app = app(config);
        let request = if method == Method::POST {
            let body = multipart_body(BOUNDARY, "policy.txt", b"data");
            multipart_request("/", BOUNDARY, body)
        } else {
            get("/test.txt")
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

#[tokio::test]
async fn authentication_rejects_malformed_headers() {
    let temp_dir = TempDir::new().unwrap();
    let public_dir = temp_dir.path();

    fs::write(public_dir.join("protected.txt"), "protected content").unwrap();

    let mut config = base_config(public_dir);
    config.security = SecurityConfig {
        username: Some("admin".to_string()),
        password: Some("secret".to_string()),
        policy: SecurityPolicy::AuthenticateAll,
    };

    let app = app(config);

    for auth_header in [
        "Basic",
        "Basic ",
        "Basic invalid-base64",
        "Basic dGVzdA==",
        "Bearer token",
        "basic dGVzdDp0ZXN0",
        "Basic YWRtaW46c2VjcmV0O2xzIC1sYQ==",
    ] {
        let response = app
            .clone()
            .oneshot(
                axum::http::Request::builder()
                    .method(Method::GET)
                    .uri("/protected.txt")
                    .header(header::AUTHORIZATION, auth_header)
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(
            response.status(),
            StatusCode::UNAUTHORIZED,
            "Auth bypass succeeded with header: {auth_header}"
        );
    }
}

#[tokio::test]
async fn authentication_timing_consistency() {
    let temp_dir = TempDir::new().unwrap();
    let public_dir = temp_dir.path();

    fs::write(public_dir.join("test.txt"), "content").unwrap();

    let mut config = base_config(public_dir);
    config.security = SecurityConfig {
        username: Some("admin".to_string()),
        password: Some("verylongpasswordthatmightrevealtiminginformation".to_string()),
        policy: SecurityPolicy::AuthenticateAll,
    };

    let app = app(config);

    let wrong_credentials = vec![
        "wrong:password",
        "admin:wrong",
        "admin:short",
        "admin:verylongwrongpasswordthatmightrevealtiminginformation",
        "wronguser:verylongpasswordthatmightrevealtiminginformation",
        "a:b",
        "admin:",
        ":verylongpasswordthatmightrevealtiminginformation",
    ];

    for creds in &wrong_credentials {
        let auth_header = base64::prelude::BASE64_STANDARD.encode(creds);
        let response = app
            .clone()
            .oneshot(
                axum::http::Request::builder()
                    .method(Method::GET)
                    .uri("/test.txt")
                    .header(header::AUTHORIZATION, format!("Basic {auth_header}"))
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(
            response.status(),
            StatusCode::UNAUTHORIZED,
            "Inconsistent response for credentials: {creds}"
        );
    }
}

#[tokio::test]
async fn cors_preflight_allows_configured_origin() {
    let temp_dir = TempDir::new().unwrap();
    let public_dir = temp_dir.path();

    fs::write(public_dir.join("test.txt"), "content").unwrap();

    let mut config = base_config(public_dir);
    config.server.cors_origins = vec!["http://localhost:3000".to_string()];

    let app = app(config);
    let response = app
        .clone()
        .oneshot(
            axum::http::Request::builder()
                .method(Method::OPTIONS)
                .uri("/test.txt")
                .header("Origin", "http://localhost:3000")
                .header("Access-Control-Request-Method", "POST")
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let headers = response.headers();
    assert_eq!(
        headers.get("Access-Control-Allow-Origin").unwrap(),
        "http://localhost:3000"
    );
    assert!(headers.contains_key("Vary"));
}

#[tokio::test]
async fn cors_preflight_sets_allow_headers_from_request() {
    let temp_dir = TempDir::new().unwrap();
    let public_dir = temp_dir.path();

    fs::write(public_dir.join("test.txt"), "content").unwrap();

    let mut config = base_config(public_dir);
    config.server.cors_origins = vec!["http://localhost:3000".to_string()];

    let app = app(config);
    let response = app
        .clone()
        .oneshot(
            axum::http::Request::builder()
                .method(Method::OPTIONS)
                .uri("/test.txt")
                .header("Origin", "http://localhost:3000")
                .header("Access-Control-Request-Method", "POST")
                .header("Access-Control-Request-Headers", "X-Test, Content-Type")
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response
            .headers()
            .get("Access-Control-Allow-Headers")
            .unwrap(),
        "X-Test, Content-Type"
    );
}

#[tokio::test]
async fn cors_preflight_denies_unknown_origin() {
    let temp_dir = TempDir::new().unwrap();
    let public_dir = temp_dir.path();

    fs::write(public_dir.join("test.txt"), "content").unwrap();

    let mut config = base_config(public_dir);
    config.server.cors_origins = vec!["http://localhost:3000".to_string()];

    let app = app(config);
    let response = app
        .clone()
        .oneshot(
            axum::http::Request::builder()
                .method(Method::OPTIONS)
                .uri("/test.txt")
                .header("Origin", "http://evil.com")
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn cors_regular_request_sets_headers() {
    let temp_dir = TempDir::new().unwrap();
    let public_dir = temp_dir.path();

    fs::write(public_dir.join("test.txt"), "content").unwrap();

    let mut config = base_config(public_dir);
    config.server.cors_origins = vec!["https://example.com".to_string()];

    let app = app(config);
    let response = app
        .clone()
        .oneshot(
            axum::http::Request::builder()
                .method(Method::GET)
                .uri("/test.txt")
                .header("Origin", "https://example.com")
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response
            .headers()
            .get("Access-Control-Allow-Origin")
            .unwrap(),
        "https://example.com"
    );
    assert!(
        !response
            .headers()
            .contains_key("Access-Control-Allow-Headers")
    );
}

#[tokio::test]
async fn cors_headers_present_on_auth_failure() {
    let temp_dir = TempDir::new().unwrap();
    let public_dir = temp_dir.path();

    fs::write(public_dir.join("test.txt"), "content").unwrap();

    let mut config = base_config(public_dir);
    config.server.cors_origins = vec!["https://example.com".to_string()];
    config.security = SecurityConfig {
        username: Some("admin".to_string()),
        password: Some("secret".to_string()),
        policy: SecurityPolicy::AuthenticateAll,
    };

    let app = app(config);
    let response = app
        .clone()
        .oneshot(
            axum::http::Request::builder()
                .method(Method::GET)
                .uri("/test.txt")
                .header("Origin", "https://example.com")
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    assert_eq!(
        response
            .headers()
            .get("Access-Control-Allow-Origin")
            .unwrap(),
        "https://example.com"
    );
    assert_eq!(
        response.headers().get(header::WWW_AUTHENTICATE).unwrap(),
        "Basic realm=\"soop3\""
    );
}

#[tokio::test]
async fn cors_headers_present_on_payload_too_large() {
    let temp_dir = TempDir::new().unwrap();
    let public_dir = temp_dir.path();

    let mut config = base_config(public_dir);
    config.server.enable_upload = true;
    config.server.cors_origins = vec!["https://example.com".to_string()];
    config.upload = UploadConfig {
        max_request_size: 16,
        prepend_timestamp: false,
        prevent_overwrite: false,
        ..Default::default()
    };

    let app = app(config);
    let body = multipart_body(BOUNDARY, "large.txt", &vec![b'x'; 128]);
    let response = app
        .oneshot(
            axum::http::Request::builder()
                .method(Method::POST)
                .uri("/")
                .header(
                    header::CONTENT_TYPE,
                    format!("multipart/form-data; boundary={BOUNDARY}"),
                )
                .header(header::ORIGIN, "https://example.com")
                .body(axum::body::Body::from(body))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::PAYLOAD_TOO_LARGE);
    assert_eq!(
        response
            .headers()
            .get(header::ACCESS_CONTROL_ALLOW_ORIGIN)
            .unwrap(),
        "https://example.com"
    );
}

#[tokio::test]
async fn cors_wildcard_allows_any_origin() {
    let temp_dir = TempDir::new().unwrap();
    let public_dir = temp_dir.path();

    fs::write(public_dir.join("test.txt"), "content").unwrap();

    let mut config = base_config(public_dir);
    config.server.cors_origins = vec!["*".to_string()];

    let app = app(config);
    let response = app
        .oneshot(
            axum::http::Request::builder()
                .method(Method::GET)
                .uri("/test.txt")
                .header("Origin", "http://any-domain.com")
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response
            .headers()
            .get("Access-Control-Allow-Origin")
            .unwrap(),
        "http://any-domain.com"
    );
}

#[tokio::test]
async fn cors_disabled_does_not_set_headers() {
    let temp_dir = TempDir::new().unwrap();
    let public_dir = temp_dir.path();

    fs::write(public_dir.join("test.txt"), "content").unwrap();

    let app = app(base_config(public_dir));
    let response = app
        .clone()
        .oneshot(
            axum::http::Request::builder()
                .method(Method::OPTIONS)
                .uri("/test.txt")
                .header("Origin", "http://localhost:3000")
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert!(
        !response
            .headers()
            .contains_key("Access-Control-Allow-Origin")
    );
}

#[tokio::test]
async fn security_headers_are_present() {
    let temp_dir = TempDir::new().unwrap();
    let public_dir = temp_dir.path();

    fs::write(public_dir.join("test.txt"), "content").unwrap();

    let app = app(base_config(public_dir));
    let response = app.oneshot(get("/test.txt")).await.unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let headers = response.headers();
    assert_eq!(headers.get("X-Frame-Options").unwrap(), "DENY");
    assert_eq!(headers.get("X-Content-Type-Options").unwrap(), "nosniff");
    assert_eq!(headers.get("X-XSS-Protection").unwrap(), "1; mode=block");
    assert!(headers.contains_key("Content-Security-Policy"));
    assert!(headers.contains_key("Referrer-Policy"));
}

#[tokio::test]
async fn options_preflight_not_blocked_with_upload_auth() {
    let temp_dir = TempDir::new().unwrap();
    let public_dir = temp_dir.path();

    fs::write(public_dir.join("test.txt"), "content").unwrap();

    let mut config = base_config(public_dir);
    config.server.enable_upload = true;
    config.server.cors_origins = vec!["http://localhost:3000".to_string()];
    config.security = SecurityConfig {
        username: Some("admin".to_string()),
        password: Some("secret".to_string()),
        policy: SecurityPolicy::AuthenticateUpload,
    };

    let app = app(config);
    let response = app
        .oneshot(
            axum::http::Request::builder()
                .method(Method::OPTIONS)
                .uri("/")
                .header("Origin", "http://localhost:3000")
                .header("Access-Control-Request-Method", "POST")
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response
            .headers()
            .get("Access-Control-Allow-Origin")
            .unwrap(),
        "http://localhost:3000"
    );
}
