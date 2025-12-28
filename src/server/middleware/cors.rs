// CORS middleware for cross-origin requests

use axum::{
    body::Body,
    extract::{Request, State},
    http::{HeaderValue, Method, Response, StatusCode, header},
    middleware::Next,
};
use tracing::debug;

use crate::server::app::AppState;

/// handle CORS headers and preflight requests
pub async fn handle_cors(
    State(state): State<AppState>,
    request: Request,
    next: Next,
) -> Response<Body> {
    let cors_origins = &state.config.server.cors_origins;

    // if no CORS origins configured, skip CORS handling
    if cors_origins.is_empty() {
        return next.run(request).await;
    }

    let origin_header = request.headers().get(header::ORIGIN).cloned();
    let origin = origin_header.as_ref().and_then(|value| value.to_str().ok());
    let requested_headers = request
        .headers()
        .get(header::ACCESS_CONTROL_REQUEST_HEADERS)
        .cloned();

    // check if origin is allowed
    let allowed = match origin {
        Some(origin) => cors_origins
            .iter()
            .any(|allowed| allowed == "*" || allowed == origin),
        None => false,
    };

    // handle preflight OPTIONS request
    if request.method() == Method::OPTIONS {
        let mut response = Response::new(Body::empty());
        if allowed {
            *response.status_mut() = StatusCode::OK;
            add_cors_headers(
                response.headers_mut(),
                origin_header.as_ref(),
                requested_headers.as_ref(),
                true,
            );
        } else {
            *response.status_mut() = StatusCode::FORBIDDEN;
        }
        return response;
    }

    // for non-preflight requests, add CORS headers if origin is allowed
    let mut response = next.run(request).await;

    if allowed {
        add_cors_headers(
            response.headers_mut(),
            origin_header.as_ref(),
            requested_headers.as_ref(),
            false,
        );
        debug!("CORS headers added for origin: {:?}", origin);
    }

    response
}

/// add CORS headers to response
fn add_cors_headers(
    headers: &mut axum::http::HeaderMap,
    origin: Option<&HeaderValue>,
    requested_headers: Option<&HeaderValue>,
    preflight: bool,
) {
    if let Some(origin) = origin {
        headers.insert(header::ACCESS_CONTROL_ALLOW_ORIGIN, origin.clone());
        headers.insert(header::VARY, HeaderValue::from_static("Origin"));
    }

    if preflight {
        headers.insert(
            header::ACCESS_CONTROL_ALLOW_METHODS,
            HeaderValue::from_static("GET, HEAD, POST, OPTIONS"),
        );

        if let Some(value) = requested_headers {
            headers.insert(header::ACCESS_CONTROL_ALLOW_HEADERS, value.clone());
        } else {
            headers.insert(
                header::ACCESS_CONTROL_ALLOW_HEADERS,
                HeaderValue::from_static("*"),
            );
        }

        headers.insert(
            header::ACCESS_CONTROL_MAX_AGE,
            HeaderValue::from_static("3600"),
        );
    }
}
