// CORS middleware for cross-origin requests

use axum::{
    body::Body,
    extract::{Request, State},
    http::{HeaderValue, Method, Response, StatusCode},
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

    let origin = request
        .headers()
        .get("origin")
        .and_then(|h| h.to_str().ok())
        .map(|s| s.to_string());

    // check if origin is allowed
    let allowed = origin.as_ref().is_some_and(|origin| {
        cors_origins.contains(&"*".to_string()) || cors_origins.contains(origin)
    });

    // handle preflight OPTIONS request
    if request.method() == Method::OPTIONS {
        if allowed {
            let mut response = Response::builder()
                .status(StatusCode::OK)
                .body(Body::empty())
                .unwrap();

            add_cors_headers(response.headers_mut(), origin.as_deref());
            return response;
        } else {
            return Response::builder()
                .status(StatusCode::FORBIDDEN)
                .body(Body::empty())
                .unwrap();
        }
    }

    // for non-preflight requests, add CORS headers if origin is allowed
    let mut response = next.run(request).await;

    if allowed {
        add_cors_headers(response.headers_mut(), origin.as_deref());
        debug!("CORS headers added for origin: {:?}", origin);
    }

    response
}

/// add CORS headers to response
fn add_cors_headers(headers: &mut axum::http::HeaderMap, origin: Option<&str>) {
    if let Some(origin) = origin {
        headers.insert(
            "Access-Control-Allow-Origin",
            HeaderValue::from_str(origin).unwrap(),
        );
    }

    headers.insert(
        "Access-Control-Allow-Methods",
        HeaderValue::from_static("GET, POST, OPTIONS"),
    );

    headers.insert(
        "Access-Control-Allow-Headers",
        HeaderValue::from_static("*"),
    );

    headers.insert("Access-Control-Max-Age", HeaderValue::from_static("3600"));
}
