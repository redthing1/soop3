// http basic authentication middleware

use axum::{
    body::Body,
    extract::{Request, State},
    http::{HeaderValue, Method, StatusCode, header},
    middleware::Next,
    response::Response,
};
use base64::prelude::*;
use tracing::{debug, error, warn};

use crate::{
    config::{SecurityConfig, SecurityPolicy},
    server::app::AppState,
};

/// http basic authentication middleware
pub async fn authenticate_if_required(
    State(state): State<AppState>,
    request: Request,
    next: Next,
) -> Result<Response, StatusCode> {
    let needs_auth = determine_auth_requirement(&state.config.security, &request);

    if !needs_auth {
        debug!("no authentication required for this request");
        return Ok(next.run(request).await);
    }

    // check if auth is configured
    let auth_available =
        state.config.security.username.is_some() && state.config.security.password.is_some();

    if !auth_available {
        error!("authentication required but credentials not configured");
        return Err(StatusCode::INTERNAL_SERVER_ERROR);
    }

    // extract and validate credentials
    let auth_header = request
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|h| h.to_str().ok());

    let auth_header = match auth_header {
        Some(header) => header,
        None => {
            warn!("authentication required but no authorization header provided");
            return Ok(unauthorized_response());
        }
    };

    let credentials = match parse_basic_auth(auth_header) {
        Ok(creds) => creds,
        Err(e) => {
            warn!("failed to parse authorization header: {}", e);
            return Ok(unauthorized_response());
        }
    };

    if validate_credentials(&state.config.security, &credentials) {
        debug!(
            "authentication successful for user: {}",
            credentials.username
        );
        Ok(next.run(request).await)
    } else {
        warn!("authentication failed for user: {}", credentials.username);
        Ok(unauthorized_response())
    }
}

/// determine if authentication is required for this request
fn determine_auth_requirement(security_config: &SecurityConfig, request: &Request) -> bool {
    let method = request.method();

    if method == Method::OPTIONS {
        return false;
    }

    let is_download = method == Method::GET || method == Method::HEAD;

    match security_config.policy {
        SecurityPolicy::AuthenticateNone => false,
        SecurityPolicy::AuthenticateAll => true,
        SecurityPolicy::AuthenticateUpload => !is_download,
        SecurityPolicy::AuthenticateDownload => is_download,
    }
}

fn unauthorized_response() -> Response {
    let mut response = Response::new(Body::empty());
    *response.status_mut() = StatusCode::UNAUTHORIZED;
    response.headers_mut().insert(
        header::WWW_AUTHENTICATE,
        HeaderValue::from_static("Basic realm=\"soop3\""),
    );
    response
}

/// parse http basic authentication header
pub fn parse_basic_auth(auth_header: &str) -> Result<BasicCredentials, &'static str> {
    // header format: "Basic <base64-encoded-credentials>"
    let mut parts = auth_header.split_whitespace();
    let scheme = parts.next().ok_or("not a basic auth header")?;
    if !scheme.eq_ignore_ascii_case("basic") {
        return Err("not a basic auth header");
    }
    let auth_header = parts.next().ok_or("missing basic auth credentials")?;
    if parts.next().is_some() {
        return Err("invalid basic auth header");
    }

    // decode base64
    let decoded = BASE64_STANDARD
        .decode(auth_header)
        .map_err(|_| "invalid base64 encoding")?;

    let decoded_str = String::from_utf8(decoded).map_err(|_| "invalid utf8 in credentials")?;

    // split on first colon
    let (username, password) = decoded_str
        .split_once(':')
        .ok_or("invalid credential format")?;

    Ok(BasicCredentials {
        username: username.to_string(),
        password: password.to_string(),
    })
}

/// validate credentials against configuration
pub fn validate_credentials(
    security_config: &SecurityConfig,
    credentials: &BasicCredentials,
) -> bool {
    let expected_username = match &security_config.username {
        Some(username) => username,
        None => return false,
    };

    let expected_password = match &security_config.password {
        Some(password) => password,
        None => return false,
    };

    // constant-time comparison to prevent timing attacks
    constant_time_eq(
        credentials.username.as_bytes(),
        expected_username.as_bytes(),
    ) && constant_time_eq(
        credentials.password.as_bytes(),
        expected_password.as_bytes(),
    )
}

/// constant-time string comparison to prevent timing attacks
pub fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }

    let mut result = 0u8;
    for (x, y) in a.iter().zip(b.iter()) {
        result |= x ^ y;
    }

    result == 0
}

/// basic authentication credentials
#[derive(Debug)]
pub struct BasicCredentials {
    pub username: String,
    pub password: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn basic_auth_parsing() {
        let header = "Basic dGVzdDp0ZXN0";
        let credentials = parse_basic_auth(header).unwrap();
        assert_eq!(credentials.username, "test");
        assert_eq!(credentials.password, "test");

        let header = "Basic dXNlcjpwYXNzQDEyMw==";
        let credentials = parse_basic_auth(header).unwrap();
        assert_eq!(credentials.username, "user");
        assert_eq!(credentials.password, "pass@123");

        let header = "basic dGVzdDp0ZXN0";
        let credentials = parse_basic_auth(header).unwrap();
        assert_eq!(credentials.username, "test");
        assert_eq!(credentials.password, "test");

        let header = "BASIC\tdGVzdDp0ZXN0";
        let credentials = parse_basic_auth(header).unwrap();
        assert_eq!(credentials.username, "test");
        assert_eq!(credentials.password, "test");

        let header = "Basic    dGVzdDp0ZXN0";
        let credentials = parse_basic_auth(header).unwrap();
        assert_eq!(credentials.username, "test");
        assert_eq!(credentials.password, "test");

        assert!(parse_basic_auth("Bearer token").is_err());
        assert!(parse_basic_auth("Basic").is_err());
        assert!(parse_basic_auth("Basic token extra").is_err());
        assert!(parse_basic_auth("Basic invalid-base64").is_err());
        assert!(parse_basic_auth("Basic dGVzdA==").is_err());
    }

    #[test]
    fn credential_validation() {
        let security_config = SecurityConfig {
            username: Some("admin".to_string()),
            password: Some("secret".to_string()),
            policy: SecurityPolicy::AuthenticateAll,
        };

        let valid_creds = BasicCredentials {
            username: "admin".to_string(),
            password: "secret".to_string(),
        };
        assert!(validate_credentials(&security_config, &valid_creds));

        let invalid_user = BasicCredentials {
            username: "wrong".to_string(),
            password: "secret".to_string(),
        };
        assert!(!validate_credentials(&security_config, &invalid_user));

        let invalid_pass = BasicCredentials {
            username: "admin".to_string(),
            password: "wrong".to_string(),
        };
        assert!(!validate_credentials(&security_config, &invalid_pass));

        let empty_creds = BasicCredentials {
            username: "".to_string(),
            password: "".to_string(),
        };
        assert!(!validate_credentials(&security_config, &empty_creds));
    }

    #[test]
    fn constant_time_comparison() {
        assert!(constant_time_eq(b"hello", b"hello"));
        assert!(!constant_time_eq(b"hello", b"world"));
        assert!(!constant_time_eq(b"hello", b"hell"));
        assert!(!constant_time_eq(b"hell", b"hello"));
        assert!(!constant_time_eq(b"", b"hello"));
        assert!(constant_time_eq(b"", b""));
    }
}
