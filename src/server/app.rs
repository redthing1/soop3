// axum application setup and server startup

use anyhow::{Context, Result};
use axum::{
    Router,
    extract::DefaultBodyLimit,
    middleware,
    routing::{get, post},
};
use std::net::SocketAddr;
use std::sync::Arc;
use tower_http::trace::TraceLayer;
use tracing::{debug, info, warn};

use super::{
    handlers::{
        assets::serve_static_asset,
        files::{handle_request, handle_root_request},
        upload::{handle_root_upload_request, handle_upload_request},
    },
    middleware::{
        auth::authenticate_if_required, cors::handle_cors, security::add_security_headers,
    },
};
use crate::config::AppConfig;

/// shared application state
#[derive(Debug, Clone)]
pub struct AppState {
    pub config: Arc<AppConfig>,
}

impl AppState {
    pub fn new(config: AppConfig) -> Self {
        Self {
            config: Arc::new(config),
        }
    }
}

/// create the axum application with all routes and middleware
pub fn create_app(config: AppConfig) -> Router {
    create_app_impl(config)
}

/// create app for testing
#[cfg(feature = "test-helpers")]
#[allow(dead_code)]
pub fn create_test_app(config: AppConfig) -> Router {
    create_app_impl(config)
}

/// internal implementation for app creation
fn create_app_impl(config: AppConfig) -> Router {
    let app_state = AppState::new(config);
    let body_limit =
        usize::try_from(app_state.config.upload.max_request_size).unwrap_or(usize::MAX);

    Router::new()
        // static asset routes
        .route("/__soop_static/{*path}", get(serve_static_asset))
        // root route
        .route("/", get(handle_root_request))
        .route("/", post(handle_root_upload_request))
        // file upload routes
        .route("/{*path}", post(handle_upload_request))
        // main file serving route
        .route("/{*path}", get(handle_request))
        // middleware stack
        .layer(middleware::from_fn_with_state(
            app_state.clone(),
            authenticate_if_required,
        ))
        .layer(middleware::from_fn_with_state(
            app_state.clone(),
            handle_cors,
        ))
        .layer(middleware::from_fn(add_security_headers))
        .layer(DefaultBodyLimit::max(body_limit))
        .layer(TraceLayer::new_for_http())
        .with_state(app_state)
}

/// start the http server
pub async fn start_server(config: AppConfig) -> Result<()> {
    let app = create_app(config.clone());

    // resolve hostname to socket address
    let host_port = format!("{}:{}", config.server.host, config.server.port);
    let addrs: Vec<SocketAddr> = tokio::net::lookup_host(&host_port)
        .await
        .with_context(|| format!("failed to resolve hostname: {host_port}"))?
        .collect();

    if addrs.is_empty() {
        return Err(anyhow::anyhow!(
            "hostname '{}' did not resolve to any addresses",
            config.server.host
        ));
    }

    // use the first resolved address
    let addr = addrs[0];

    // log hostname resolution if not an IP address
    if config.server.host.parse::<std::net::IpAddr>().is_err() {
        debug!("resolved hostname '{}' to {}", config.server.host, addr);
    }

    // log startup information
    info!(
        "starting soop3 v{} at http://{}:{}",
        env!("CARGO_PKG_VERSION"),
        config.server.host,
        config.server.port
    );
    info!("public dir: {}", config.server.public_dir.display());

    if config.server.enable_upload {
        info!(
            "uploads enabled, saving to: {}",
            config.upload_dir().display()
        );
        warn!("file uploads are enabled - ensure proper security measures");
    }

    // start the server
    let listener = tokio::net::TcpListener::bind(&addr)
        .await
        .context("failed to bind to address")?;

    info!("server listening on {}", addr);

    axum::serve(listener, app).await.context("server error")?;

    Ok(())
}
