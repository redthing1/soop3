// http basic authentication middleware

use axum::{
    extract::{Request, State},
    http::{StatusCode, header},
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
            return Err(StatusCode::UNAUTHORIZED);
        }
    };

    let credentials = match parse_basic_auth(auth_header) {
        Ok(creds) => creds,
        Err(e) => {
            warn!("failed to parse authorization header: {}", e);
            return Err(StatusCode::UNAUTHORIZED);
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
        Err(StatusCode::UNAUTHORIZED)
    }
}

/// determine if authentication is required for this request
fn determine_auth_requirement(security_config: &SecurityConfig, request: &Request) -> bool {
    let is_upload = request.method() != axum::http::Method::GET;

    match security_config.policy {
        SecurityPolicy::AuthenticateNone => false,
        SecurityPolicy::AuthenticateAll => true,
        SecurityPolicy::AuthenticateUpload => is_upload,
        SecurityPolicy::AuthenticateDownload => !is_upload,
    }
}

/// parse http basic authentication header
pub fn parse_basic_auth(auth_header: &str) -> Result<BasicCredentials, &'static str> {
    // header format: "Basic <base64-encoded-credentials>"
    let auth_header = auth_header
        .strip_prefix("Basic ")
        .ok_or("not a basic auth header")?;

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
