// axum application setup and server startup

use std::net::SocketAddr;
use std::sync::Arc;
use anyhow::{Context, Result};
use axum::{Router, routing::{get, post}, middleware};
use tower::ServiceBuilder;
use tower_http::trace::TraceLayer;
use tracing::{info, warn};

use crate::config::AppConfig;
use super::{
    handlers::{
        files::{handle_request, handle_root_request},
        assets::serve_static_asset,
        upload::{handle_upload_request, handle_root_upload_request},
    },
    middleware::{auth::authenticate_if_required, security::add_security_headers},
};

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
    create_app_impl(config, true)
}

/// create app for testing (skips validation)
pub fn create_test_app(config: AppConfig) -> Router {
    create_app_impl(config, false)
}

/// internal implementation for app creation
fn create_app_impl(config: AppConfig, validate: bool) -> Router {
    if validate {
        // in production, we expect configuration to be pre-validated
        // but for testing, we might want to skip validation
    }
    let app_state = AppState::new(config);
    
    Router::new()
        // static asset routes
        .route("/__soop_static/*path", get(serve_static_asset))
        
        // root route
        .route("/", get(handle_root_request))
        .route("/", post(handle_root_upload_request))
        // file upload routes
        .route("/*path", post(handle_upload_request))
        // main file serving route
        .route("/*path", get(handle_request))
        
        // middleware stack
        .layer(
            ServiceBuilder::new()
                .layer(TraceLayer::new_for_http())
                .layer(middleware::from_fn(add_security_headers))
        )
        .layer(middleware::from_fn_with_state(
            app_state.clone(),
            authenticate_if_required,
        ))
        
        .with_state(app_state)
}

/// start the http server
pub async fn start_server(config: AppConfig) -> Result<()> {
    let app = create_app(config.clone());
    
    // create socket address
    let addr: SocketAddr = format!("{}:{}", config.server.host, config.server.port)
        .parse()
        .context("invalid host/port combination")?;
    
    // log startup information
    info!("starting soop3 v{} at http://{}", 
          env!("CARGO_PKG_VERSION"), addr);
    info!("public dir: {}", config.server.public_dir.display());
    
    if config.server.enable_upload {
        info!("uploads enabled, saving to: {}", config.upload_dir().display());
        warn!("file uploads are enabled - ensure proper security measures");
    }
    
    // start the server
    let listener = tokio::net::TcpListener::bind(&addr).await
        .context("failed to bind to address")?;
    
    info!("server listening on {}", addr);
    
    axum::serve(listener, app).await
        .context("server error")?;
    
    Ok(())
}