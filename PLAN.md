# PLAN.md for soop3 (Rust Port)

## project overview

**soop3** is a high-performance, memory-safe port of soop2 from D to Rust. We maintain all original functionality while leveraging Rust's zero-cost abstractions and safety guarantees.

**core philosophy:**
- preserve all soop2 features and behavior exactly
- use high-quality abstractions to reduce complexity
- maintain elegant, self-documenting code organization
- prioritize performance and memory safety
- lowercase comments for consistency
- well-thought-out architecture with clear separation of concerns

## original soop2 analysis

### key features preserved
- http file browsing with minimalist ui
- http file upload (post) support
- configuration from toml file
- support for http basic auth for uploads/downloads/both
- cli tool interface identical to original
- configurable server with same toml format

### technical architecture (d → rust)
- **web framework**: vibrant.d → axum + tower
- **cli parsing**: commandr → clap with derive macros
- **config**: toml parsing → serde + figment
- **logging**: minlog → tracing ecosystem
- **assets**: embedded static files → rust-embed
- **utilities**: custom d code → idiomatic rust implementations

## library selection rationale

### http server: axum + tower + tokio

**axum** (primary web framework)
```rust
// elegant request handling with type-safe extractors
pub async fn serve_file(
    State(config): State<AppConfig>,
    Path(file_path): Path<String>
) -> Result<Response, AppError> {
    // self-documenting function signature
    // zero boilerplate request parsing
}
```

**justification:**
- **high-quality abstractions**: built on tower's service/middleware model
- **elegant api**: type-safe extractors eliminate manual request parsing
- **performance**: zero-cost abstractions over hyper/tokio
- **ecosystem**: first-class integration with tower middleware
- **maintainability**: excellent error messages, clear documentation
- **familiarity**: similar patterns to vibrant.d routing

**tower** (middleware composition)
- **elegant composition**: functional middleware composition
- **rich ecosystem**: auth, compression, logging, rate limiting
- **type-safe**: compile-time service composition
- **matches original**: auth and logging middleware patterns

**tokio** (async runtime)
- **performance**: best-in-class async runtime, better than d's fibers
- **ecosystem**: de facto standard, excellent ecosystem integration
- **abstractions**: high-level apis (fs, net) with zero-cost async

### configuration: serde + toml + figment

**serde + toml** (core configuration)
```rust
#[derive(Deserialize, Debug, Clone)]
pub struct ServerConfig {
    pub host: Option<String>,          // cli override support
    pub port: Option<u16>,             // optional with sensible defaults
    pub public_dir: Option<PathBuf>,   // type-safe paths
    pub enable_upload: Option<bool>,   // explicit option types
}

#[derive(Deserialize, Debug, Clone)]
pub struct SecurityConfig {
    pub username: Option<String>,
    pub password: Option<String>,
    pub policy: SecurityPolicy,
}

#[derive(Deserialize, Debug, Clone)]
pub enum SecurityPolicy {
    AuthenticateNone,
    AuthenticateUpload,
    AuthenticateDownload,
    AuthenticateAll,
}
```

**justification:**
- **zero boilerplate**: derive macros eliminate manual parsing code
- **type safety**: compile-time guarantees vs d's runtime toml parsing
- **elegant**: clean struct mapping with optional fields
- **maintains compatibility**: exact same toml format as soop2

**figment** (configuration merging)
```rust
// elegant merging: config file < environment < cli args
let config: AppConfig = Figment::new()
    .merge(Toml::file_exact("config.toml"))
    .merge(Env::prefixed("SOOP_"))
    .merge(Serialized::defaults(cli_args))
    .extract()?;
```

**justification:**
- **elegant abstraction**: handles complex precedence rules cleanly
- **maintains pattern**: cli args override config file seamlessly
- **rich features**: environment variables, multiple sources, validation
- **zero complexity**: eliminates manual override logic

### cli parsing: clap v4 with derive macros

```rust
#[derive(Parser, Debug)]
#[command(name = "soop3", version = env!("CARGO_PKG_VERSION"))]
#[command(about = "the based http fileserver")]
pub struct Cli {
    /// public directory to serve
    #[arg(default_value = ".")]
    pub public_dir: PathBuf,
    
    /// enable file uploads
    #[arg(short = 'u', long)]
    pub enable_upload: bool,
    
    /// host to listen on
    #[arg(short = 'l', long, default_value = "0.0.0.0")]
    pub host: String,
    
    /// port to listen on
    #[arg(short = 'p', long, default_value = "8000")]
    pub port: u16,
    
    /// config file to use
    #[arg(short = 'c', long)]
    pub config_file: Option<PathBuf>,
    
    /// increase verbosity
    #[arg(short = 'v', long, action = clap::ArgAction::Count)]
    pub verbose: u8,
    
    /// decrease verbosity
    #[arg(short = 'q', long, action = clap::ArgAction::Count)]
    pub quiet: u8,
}
```

**justification:**
- **derive macros**: eliminate boilerplate vs manual commandr setup
- **self-documenting**: help text embedded directly in code
- **type safety**: automatic validation and conversion
- **exact api**: maintains soop2's cli interface precisely
- **rich features**: built-in help generation, validation, subcommands

### logging: tracing ecosystem

**tracing** (structured logging)
```rust
#[instrument(skip(state), fields(path = %request_path))]
pub async fn handle_file_request(
    state: AppState,
    request_path: String
) -> Result<Response, AppError> {
    info!("processing file request");
    
    // automatic span creation and context propagation
    let file_path = validate_path(&request_path)?;
    
    debug!("validated path: {}", file_path.display());
    
    serve_file_response(file_path).await
}
```

**justification:**
- **structured logging**: better than printf-style logging in minlog
- **async aware**: proper context propagation in concurrent operations
- **instrument macro**: automatic span creation eliminates boilerplate
- **verbosity levels**: maintains minlog's flexibility with better granularity
- **rich context**: automatic request correlation and timing

### error handling: anyhow + thiserror

**thiserror** (library errors)
```rust
#[derive(thiserror::Error, Debug)]
pub enum AppError {
    #[error("file not found: {path}")]
    FileNotFound { path: String },
    
    #[error("permission denied: {path}")]
    PermissionDenied { path: String },
    
    #[error("path traversal attempt: {path}")]
    PathTraversal { path: String },
    
    #[error("upload too large: {size} bytes (max: {max})")]
    UploadTooLarge { size: u64, max: u64 },
}
```

**anyhow** (application errors)
```rust
// ergonomic error propagation with rich context
pub async fn process_upload(
    upload_dir: &Path,
    file_data: Bytes,
    filename: &str,
) -> anyhow::Result<PathBuf> {
    let target_path = validate_upload_path(upload_dir, filename)
        .with_context(|| format!("validating upload path: {}", filename))?;
    
    write_upload_file(&target_path, file_data).await
        .with_context(|| format!("writing upload file: {}", target_path.display()))?;
    
    Ok(target_path)
}
```

**justification:**
- **ergonomic**: context chaining with ? operator
- **type safe**: custom error types with rich context
- **maintainable**: clear error messages for debugging
- **performance**: zero-cost when not erroring

### file operations: tokio::fs + walkdir

**tokio::fs** (async file operations)
```rust
// non-blocking file operations
pub async fn read_file_metadata(path: &Path) -> Result<FileMetadata, io::Error> {
    let metadata = tokio::fs::metadata(path).await?;
    
    Ok(FileMetadata {
        size: metadata.len(),
        modified: metadata.modified()?,
        is_dir: metadata.is_dir(),
        permissions: metadata.permissions(),
    })
}
```

**walkdir** (directory traversal)
```rust
// robust directory walking with error handling
pub fn collect_directory_entries(dir_path: &Path) -> Result<Vec<DirEntry>, io::Error> {
    WalkDir::new(dir_path)
        .max_depth(1)
        .into_iter()
        .filter_map(|entry| match entry {
            Ok(entry) if entry.path() != dir_path => Some(Ok(entry)),
            Ok(_) => None, // skip the directory itself
            Err(e) => Some(Err(e.into())),
        })
        .collect()
}
```

**justification:**
- **performance**: non-blocking file operations vs blocking d equivalents
- **elegant**: same api as std::fs but async
- **robust**: handles edge cases, symlinks, permissions properly
- **maintains behavior**: exactly matches original directory listing logic

### asset embedding: rust-embed

```rust
#[derive(RustEmbed)]
#[folder = "assets/"]
#[include = "*.css"]
#[include = "*.svg"]
#[include = "*.ico"]
pub struct StaticAssets;

// clean access to embedded files
pub fn get_embedded_asset(path: &str) -> Option<Cow<'static, [u8]>> {
    StaticAssets::get(path)
}

// efficient serving with proper mime types
pub async fn serve_embedded_asset(
    asset_path: &str
) -> Result<Response, AppError> {
    let asset = StaticAssets::get(asset_path)
        .ok_or_else(|| AppError::AssetNotFound { path: asset_path.to_string() })?;
    
    let mime_type = mime_guess::from_path(asset_path)
        .first_or_octet_stream();
    
    Ok(Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, mime_type.as_ref())
        .header(header::CACHE_CONTROL, "public, max-age=31536000")
        .body(Body::from(asset.data))?)
}
```

**justification:**
- **elegant**: derive macro eliminates manual include_bytes! calls
- **performance**: compile-time embedding, zero runtime cost
- **rich features**: compression, hash-based caching, selective inclusion
- **clean api**: maintains embedded asset patterns from original

### date/time: chrono

```rust
// rich date/time formatting matching original datefmt.d
pub fn format_file_timestamp(timestamp: SystemTime) -> String {
    let datetime: DateTime<Local> = timestamp.into();
    datetime.format("%Y-%m-%d %H:%M:%S").to_string()
}

// timezone-aware operations
pub fn parse_http_date(date_str: &str) -> Result<SystemTime, chrono::ParseError> {
    DateTime::parse_from_rfc2822(date_str)
        .map(|dt| dt.into())
}
```

**justification:**
- **rich abstractions**: timezone-aware, formatting, parsing built-in
- **maintains compatibility**: replaces custom datefmt.d functionality
- **elegant api**: intuitive formatting with strftime compatibility
- **performance**: efficient operations with minimal allocations

### path security: custom implementation

```rust
// robust path jailing implementation
pub fn ensure_path_within_jail(
    jail_root: &Path,
    target_path: &Path,
) -> Result<PathBuf, PathTraversalError> {
    // canonicalize both paths to resolve symlinks and relative components
    let canonical_jail = jail_root.canonicalize()
        .map_err(|_| PathTraversalError::InvalidJailRoot)?;
    
    let canonical_target = target_path.canonicalize()
        .map_err(|_| PathTraversalError::InvalidTargetPath)?;
    
    // ensure target is within jail boundaries
    if !canonical_target.starts_with(&canonical_jail) {
        return Err(PathTraversalError::OutsideJail {
            jail: canonical_jail,
            target: canonical_target,
        });
    }
    
    Ok(canonical_target)
}

// safe path joining that prevents traversal
pub fn join_path_jailed(
    base_dir: &Path,
    component: &str,
) -> Result<PathBuf, PathTraversalError> {
    // normalize component to prevent traversal
    let normalized = normalize_path_component(component)?;
    
    // join and validate
    let joined = base_dir.join(normalized);
    ensure_path_within_jail(base_dir, &joined)
}
```

**justification:**
- **security-first**: exact port of original join_path_jailed logic
- **elegant**: extension trait pattern for PathBuf operations
- **zero-trust**: prevents all known path traversal attack vectors
- **type-safe**: compile-time guarantees about path safety

## code organization

```
soop3/
├── Cargo.toml                 # project configuration and dependencies
├── PLAN.md                    # this comprehensive plan
├── README.md                  # user documentation
├── assets/                    # static files for embedding
│   ├── style.css             # directory listing styles
│   ├── icon.svg              # application icon
│   ├── favicon.ico           # browser favicon
│   └── brand.svg             # branding element
├── src/
│   ├── main.rs               # entry point, minimal bootstrap
│   ├── lib.rs                # library root for testing
│   ├── config/
│   │   ├── mod.rs            # configuration module public api
│   │   ├── types.rs          # config struct definitions
│   │   ├── loading.rs        # toml parsing and merging logic
│   │   └── validation.rs     # config validation rules
│   ├── server/
│   │   ├── mod.rs            # server module public api
│   │   ├── app.rs            # axum app creation and startup
│   │   ├── handlers/
│   │   │   ├── mod.rs        # handler module exports
│   │   │   ├── files.rs      # file serving handlers
│   │   │   ├── upload.rs     # file upload handlers
│   │   │   ├── listing.rs    # directory listing handlers
│   │   │   └── assets.rs     # static asset handlers
│   │   ├── middleware/
│   │   │   ├── mod.rs        # middleware module exports
│   │   │   ├── auth.rs       # http basic authentication
│   │   │   ├── logging.rs    # request logging and tracing
│   │   │   └── errors.rs     # error handling and responses
│   │   └── extractors.rs     # custom axum extractors
│   ├── core/
│   │   ├── mod.rs            # core business logic
│   │   ├── auth.rs           # authentication logic
│   │   ├── upload.rs         # upload processing
│   │   ├── listing.rs        # directory listing generation
│   │   └── mime.rs           # mime type detection
│   └── utils/
│       ├── mod.rs            # utility module exports
│       ├── paths.rs          # path operations and security
│       ├── files.rs          # file operations and formatting
│       ├── filters.rs        # gitignore-style filtering
│       └── html.rs           # html generation utilities
├── tests/
│   ├── integration/          # integration tests
│   │   ├── server_tests.rs   # full server testing
│   │   ├── upload_tests.rs   # upload functionality
│   │   └── auth_tests.rs     # authentication testing
│   └── fixtures/             # test data and fixtures
└── benches/                  # performance benchmarks
    └── server_bench.rs       # throughput and latency benchmarks
```

**organization principles:**
- **clear separation**: each module has single responsibility
- **self-documenting**: module names clearly explain purpose
- **logical grouping**: related functionality grouped together
- **minimal public apis**: expose only what's needed externally
- **testable**: structure supports comprehensive testing

## implementation phases

### phase 1: foundation (high priority)

#### 1.1 project setup
```toml
# Cargo.toml - carefully curated dependencies
[package]
name = "soop3"
version = "0.9.0"
edition = "2021"
authors = ["redthing1"]
description = "the based http fileserver (rust port)"
license = "proprietary"

[dependencies]
# web server foundation
axum = { version = "0.7", features = ["multipart", "tower-log"] }
tokio = { version = "1.0", features = ["full"] }
tower = { version = "0.4", features = ["full"] }
tower-http = { version = "0.5", features = ["fs", "trace", "auth"] }

# configuration and cli
clap = { version = "4.0", features = ["derive", "env"] }
serde = { version = "1.0", features = ["derive"] }
toml = "0.8"
figment = { version = "0.10", features = ["toml", "env"] }

# error handling and logging
anyhow = "1.0"
thiserror = "1.0"
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter"] }

# file operations and utilities
walkdir = "2.0"
mime_guess = "2.0"
chrono = { version = "0.4", features = ["serde"] }
rust-embed = { version = "8.0", features = ["compression"] }

# security and validation
base64 = "0.21"
percent-encoding = "2.0"

[dev-dependencies]
tempfile = "3.0"
tokio-test = "0.4"
```

#### 1.2 basic project structure
- create module hierarchy
- set up basic error types
- implement logging initialization
- create placeholder types

#### 1.3 configuration system
```rust
// src/config/types.rs - comprehensive config types
#[derive(Debug, Clone, Deserialize)]
pub struct AppConfig {
    pub server: ServerConfig,
    pub security: SecurityConfig,
    pub listing: ListingConfig,
    pub upload: UploadConfig,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ServerConfig {
    pub host: String,
    pub port: u16,
    pub public_dir: PathBuf,
    pub upload_dir: Option<PathBuf>,
    pub enable_upload: bool,
}

// src/config/loading.rs - elegant config merging
pub fn load_configuration(cli: &Cli) -> Result<AppConfig, ConfigError> {
    let mut figment = Figment::new()
        .merge(Serialized::defaults(AppConfig::default()));
    
    // merge config file if provided
    if let Some(config_path) = &cli.config_file {
        figment = figment.merge(Toml::file(config_path));
    }
    
    // cli overrides take precedence
    figment = figment.merge(Serialized::defaults(cli));
    
    figment.extract().map_err(ConfigError::from)
}
```

#### 1.4 cli interface
```rust
// src/main.rs - clean entry point
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    
    // initialize logging based on verbosity
    init_logging(cli.verbose, cli.quiet)?;
    
    // load and validate configuration
    let config = load_configuration(&cli)?;
    
    // start server
    start_server(config).await
}
```

### phase 2: core server (high priority)

#### 2.1 basic http server
```rust
// src/server/app.rs - axum application setup
pub fn create_app(config: AppConfig) -> Router {
    let app_state = AppState::new(config);
    
    Router::new()
        // api routes
        .route("/*path", get(handle_get_request))
        .route("/*path", post(handle_post_request))
        
        // internal static assets
        .route("/__soop_static/*path", get(handle_static_asset))
        
        // middleware stack
        .layer(middleware::from_fn(log_requests))
        .layer(middleware::from_fn_with_state(
            app_state.clone(),
            authenticate_if_required
        ))
        .layer(tower_http::trace::TraceLayer::new_for_http())
        
        .with_state(app_state)
}

pub async fn start_server(config: AppConfig) -> anyhow::Result<()> {
    let app = create_app(config.clone());
    
    let addr = SocketAddr::new(
        config.server.host.parse()?,
        config.server.port
    );
    
    info!("starting soop3 v{} at http://{}", 
          env!("CARGO_PKG_VERSION"), addr);
    info!("public dir: {}", config.server.public_dir.display());
    
    if config.server.enable_upload {
        info!("uploads enabled, saving to: {}", 
              config.upload_dir().display());
    }
    
    axum::Server::bind(&addr)
        .serve(app.into_make_service())
        .await?;
    
    Ok(())
}
```

#### 2.2 file serving handlers
```rust
// src/server/handlers/files.rs - elegant file serving
#[instrument(skip(state), fields(path = %file_path))]
pub async fn handle_get_request(
    State(state): State<AppState>,
    Path(file_path): Path<String>
) -> Result<Response, AppError> {
    info!("processing get request");
    
    // validate and resolve path securely
    let resolved_path = resolve_safe_path(
        &state.config.server.public_dir,
        &file_path
    )?;
    
    // check if path exists
    if !resolved_path.exists() {
        return Ok(not_found_response());
    }
    
    if resolved_path.is_dir() {
        handle_directory_request(state, resolved_path, file_path).await
    } else {
        handle_file_request(state, resolved_path).await
    }
}

async fn handle_file_request(
    state: AppState,
    file_path: PathBuf
) -> Result<Response, AppError> {
    let file = tokio::fs::File::open(&file_path).await?;
    let metadata = file.metadata().await?;
    
    // determine mime type
    let mime_type = mime_guess::from_path(&file_path)
        .first_or_octet_stream();
    
    // create streaming response
    let stream = ReaderStream::new(file);
    let body = Body::from_stream(stream);
    
    Ok(Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, mime_type.as_ref())
        .header(header::CONTENT_LENGTH, metadata.len())
        .body(body)?)
}
```

#### 2.3 directory listing generation
```rust
// src/server/handlers/listing.rs - html directory listing
pub async fn generate_directory_listing(
    config: &AppConfig,
    dir_path: &Path,
    request_path: &str,
) -> Result<Html<String>, AppError> {
    // collect directory entries
    let entries = collect_directory_entries(dir_path).await?;
    
    // apply filtering rules
    let filtered_entries = apply_listing_filters(entries, config)?;
    
    // sort entries (directories first, then alphabetical)
    let mut sorted_entries = filtered_entries;
    sorted_entries.sort_by(|a, b| {
        match (a.is_dir(), b.is_dir()) {
            (true, false) => Ordering::Less,
            (false, true) => Ordering::Greater,
            _ => a.file_name().cmp(&b.file_name()),
        }
    });
    
    // generate html
    let html = build_listing_html(&sorted_entries, request_path, config)?;
    Ok(Html(html))
}

fn build_listing_html(
    entries: &[DirEntry],
    request_path: &str,
    config: &AppConfig,
) -> Result<String, AppError> {
    let mut html = String::new();
    
    // html document structure
    html.push_str("<!DOCTYPE html>");
    html.push_str("<html><head>");
    html.push_str("<meta charset=\"utf-8\">");
    html.push_str("<meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">");
    html.push_str(&format!(
        "<meta name=\"generator\" content=\"soop3 v{}\">",
        env!("CARGO_PKG_VERSION")
    ));
    html.push_str("<link rel=\"icon\" href=\"/__soop_static/icon.svg\">");
    html.push_str(&format!("<title>soop3 | {}</title>", request_path));
    html.push_str("<link rel=\"stylesheet\" href=\"/__soop_static/style.css\">");
    html.push_str("</head><body>");
    
    // content structure
    html.push_str("<div class=\"wrapper\">");
    html.push_str("<main>");
    html.push_str("<a href=\"/\"><img src=\"/__soop_static/icon.svg\" alt=\"logo\" class=\"logo-icon\"></a>");
    html.push_str(&format!(
        "<h1 class=\"index-info\">Index of <code>{}</code></h1>",
        escape_html(request_path)
    ));
    
    // file listing table
    html.push_str("<table class=\"list\">");
    html.push_str("<tr><th>name</th><th>size</th><th>modified</th></tr>");
    
    // parent directory link
    if request_path != "/" {
        html.push_str("<tr><td><a href=\"../\">../</a></td><td></td><td></td></tr>");
    }
    
    // directory entries
    for entry in entries {
        let entry_html = format_directory_entry(entry, request_path)?;
        html.push_str(&entry_html);
    }
    
    html.push_str("</table>");
    html.push_str("</main>");
    html.push_str(&format!(
        "<footer><p>Generated by <code>soop3 v{}</code></p></footer>",
        env!("CARGO_PKG_VERSION")
    ));
    html.push_str("</div></body></html>");
    
    Ok(html)
}
```

### phase 3: advanced features (medium priority)

#### 3.1 file upload implementation
```rust
// src/server/handlers/upload.rs - secure file uploads
#[instrument(skip(state, multipart), fields(path = %upload_path))]
pub async fn handle_post_request(
    State(state): State<AppState>,
    Path(upload_path): Path<String>,
    mut multipart: Multipart,
) -> Result<StatusCode, AppError> {
    info!("processing upload request");
    
    // verify uploads are enabled
    if !state.config.server.enable_upload {
        return Err(AppError::UploadsDisabled);
    }
    
    // extract uploaded file
    let file_data = extract_upload_file(&mut multipart).await?;
    
    // validate and process upload
    let target_path = process_upload(
        &state.config,
        &upload_path,
        file_data,
    ).await?;
    
    info!("upload completed: {}", target_path.display());
    Ok(StatusCode::NO_CONTENT)
}

async fn process_upload(
    config: &AppConfig,
    upload_path: &str,
    file_data: UploadedFile,
) -> Result<PathBuf, AppError> {
    // determine target filename
    let filename = if config.upload.prepend_timestamp {
        let timestamp = chrono::Utc::now().format("%Y%m%d_%H%M%S");
        format!("{}_{}", timestamp, upload_path)
    } else {
        upload_path.to_string()
    };
    
    // validate target path
    let target_path = validate_upload_path(
        config.upload_dir(),
        &filename,
    )?;
    
    // ensure parent directory exists
    if let Some(parent) = target_path.parent() {
        if !parent.exists() {
            if config.upload.create_directories {
                tokio::fs::create_dir_all(parent).await?;
            } else {
                return Err(AppError::DirectoryNotFound {
                    path: parent.to_path_buf(),
                });
            }
        }
    }
    
    // check for existing file
    if target_path.exists() && config.upload.prevent_overwrite {
        return Err(AppError::FileExists {
            path: target_path,
        });
    }
    
    // write file atomically
    write_upload_file(&target_path, file_data.data).await?;
    
    Ok(target_path)
}
```

#### 3.2 authentication middleware
```rust
// src/server/middleware/auth.rs - http basic authentication
pub async fn authenticate_if_required<B>(
    State(state): State<AppState>,
    request: Request<B>,
    next: Next<B>,
) -> Result<Response, AppError> {
    let needs_auth = determine_auth_requirement(&state.config, &request);
    
    if !needs_auth {
        return Ok(next.run(request).await);
    }
    
    // extract and validate credentials
    let auth_header = request.headers()
        .get(header::AUTHORIZATION)
        .and_then(|h| h.to_str().ok())
        .ok_or(AppError::MissingAuth)?;
    
    let credentials = parse_basic_auth(auth_header)?;
    validate_credentials(&state.config.security, &credentials)?;
    
    Ok(next.run(request).await)
}

fn determine_auth_requirement<B>(
    config: &AppConfig,
    request: &Request<B>,
) -> bool {
    let is_upload = request.method() != Method::GET;
    
    match config.security.policy {
        SecurityPolicy::AuthenticateNone => false,
        SecurityPolicy::AuthenticateAll => true,
        SecurityPolicy::AuthenticateUpload => is_upload,
        SecurityPolicy::AuthenticateDownload => !is_upload,
    }
}

fn validate_credentials(
    security_config: &SecurityConfig,
    credentials: &BasicCredentials,
) -> Result<(), AppError> {
    let expected_username = security_config.username.as_ref()
        .ok_or(AppError::AuthNotConfigured)?;
    let expected_password = security_config.password.as_ref()
        .ok_or(AppError::AuthNotConfigured)?;
    
    // constant-time comparison to prevent timing attacks
    let username_valid = constant_time_eq(
        credentials.username.as_bytes(),
        expected_username.as_bytes(),
    );
    let password_valid = constant_time_eq(
        credentials.password.as_bytes(),
        expected_password.as_bytes(),
    );
    
    if username_valid && password_valid {
        Ok(())
    } else {
        Err(AppError::InvalidCredentials)
    }
}
```

### phase 4: utilities and polish (medium priority)

#### 4.1 path security implementation
```rust
// src/utils/paths.rs - comprehensive path security
use std::path::{Path, PathBuf, Component};

pub fn join_path_jailed(
    base_dir: &Path,
    component: &str,
) -> Result<PathBuf, PathTraversalError> {
    // normalize component to prevent traversal
    let normalized = normalize_path_component(component)?;
    
    // join paths
    let joined = base_dir.join(normalized);
    
    // canonicalize and validate
    let canonical_base = base_dir.canonicalize()
        .map_err(|_| PathTraversalError::InvalidBasePath)?;
    
    let canonical_joined = joined.canonicalize()
        .map_err(|_| PathTraversalError::InvalidTargetPath)?;
    
    // ensure result is within jail
    if !canonical_joined.starts_with(&canonical_base) {
        return Err(PathTraversalError::OutsideJail {
            base: canonical_base,
            target: canonical_joined,
        });
    }
    
    Ok(canonical_joined)
}

fn normalize_path_component(component: &str) -> Result<PathBuf, PathTraversalError> {
    // url decode the component
    let decoded = percent_encoding::percent_decode_str(component)
        .decode_utf8()
        .map_err(|_| PathTraversalError::InvalidEncoding)?;
    
    // build normalized path
    let mut normalized = PathBuf::new();
    
    for component in Path::new(decoded.as_ref()).components() {
        match component {
            Component::Normal(name) => normalized.push(name),
            Component::CurDir => {}, // ignore "."
            Component::ParentDir => {
                // allow going up, but validation will catch jail escapes
                normalized.push("..");
            },
            Component::RootDir => {
                // start fresh from root
                normalized = PathBuf::from("/");
            },
            Component::Prefix(_) => {
                // windows drive prefixes not allowed
                return Err(PathTraversalError::WindowsPrefix);
            },
        }
    }
    
    Ok(normalized)
}
```

#### 4.2 file utilities
```rust
// src/utils/files.rs - file operations and formatting
pub fn format_file_size(size: u64) -> String {
    const UNITS: &[&str] = &["B", "KiB", "MiB", "GiB", "TiB", "PiB"];
    const THRESHOLD: f64 = 1024.0;
    
    if size == 0 {
        return "0 B".to_string();
    }
    
    let mut size_f = size as f64;
    let mut unit_index = 0;
    
    while size_f >= THRESHOLD && unit_index < UNITS.len() - 1 {
        size_f /= THRESHOLD;
        unit_index += 1;
    }
    
    if unit_index == 0 {
        format!("{} {}", size, UNITS[unit_index])
    } else {
        format!("{:.1} {}", size_f, UNITS[unit_index])
    }
}

pub async fn collect_directory_entries(
    dir_path: &Path
) -> Result<Vec<DirectoryEntry>, io::Error> {
    let mut entries = Vec::new();
    let mut read_dir = tokio::fs::read_dir(dir_path).await?;
    
    while let Some(entry) = read_dir.next_entry().await? {
        let metadata = entry.metadata().await?;
        let file_name = entry.file_name();
        
        entries.push(DirectoryEntry {
            name: file_name.to_string_lossy().into_owned(),
            path: entry.path(),
            size: metadata.len(),
            modified: metadata.modified()?,
            is_dir: metadata.is_dir(),
        });
    }
    
    Ok(entries)
}
```

#### 4.3 comprehensive testing
```rust
// tests/integration/server_tests.rs - integration testing
#[tokio::test]
async fn test_file_serving() {
    let temp_dir = create_test_directory().await;
    let config = create_test_config(&temp_dir);
    let app = create_app(config);
    
    // test file serving
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
    
    let body = hyper::body::to_bytes(response.into_body()).await.unwrap();
    assert_eq!(body, "test content");
}

#[tokio::test]
async fn test_directory_listing() {
    let temp_dir = create_test_directory().await;
    let config = create_test_config(&temp_dir);
    let app = create_app(config);
    
    let response = app
        .oneshot(
            Request::builder()
                .uri("/")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    
    assert_eq!(response.status(), StatusCode::OK);
    
    let body = hyper::body::to_bytes(response.into_body()).await.unwrap();
    let html = String::from_utf8(body.to_vec()).unwrap();
    
    assert!(html.contains("Index of"));
    assert!(html.contains("test.txt"));
}

#[tokio::test]
async fn test_path_traversal_prevention() {
    let temp_dir = create_test_directory().await;
    let config = create_test_config(&temp_dir);
    let app = create_app(config);
    
    // attempt path traversal
    let response = app
        .oneshot(
            Request::builder()
                .uri("/../../../etc/passwd")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}
```

## coding standards

### style guidelines
```rust
// lowercase comments throughout codebase
// self-documenting function and variable names
// clear intent over cleverness
// comprehensive error handling

pub async fn serve_directory_listing(
    config: &ListingConfig,
    directory_path: &Path,
    request_path: &str,
) -> Result<Html<String>, AppError> {
    // validate path is within allowed boundaries
    let canonical_path = validate_and_canonicalize_path(directory_path)?;
    
    // collect directory entries with metadata
    let entries = collect_directory_entries(&canonical_path).await?;
    
    // apply filtering rules if configured
    let filtered_entries = apply_listing_filters(entries, config)?;
    
    // generate html response
    build_directory_listing_html(filtered_entries, request_path)
}

// consistent error handling patterns
pub fn validate_upload_path(
    base_dir: &Path,
    upload_path: &str,
) -> Result<PathBuf, PathValidationError> {
    // prevent directory traversal attacks
    let normalized = normalize_path_component(upload_path)?;
    
    // ensure path stays within jail
    let full_path = base_dir.join(normalized);
    ensure_path_within_jail(base_dir, &full_path)?;
    
    Ok(full_path)
}

// comprehensive documentation
/// validates and normalizes a file upload path
/// 
/// # arguments
/// * `base_dir` - the base directory for uploads (jail root)
/// * `upload_path` - the requested upload path from client
/// 
/// # returns
/// * `ok(pathbuf)` - validated and jailed path
/// * `err(pathvalidationerror)` - path validation failed
/// 
/// # security
/// this function prevents directory traversal attacks by:
/// - normalizing path components
/// - ensuring result stays within base_dir
/// - canonicalizing paths to resolve symlinks
pub fn validate_upload_path(
    base_dir: &Path,
    upload_path: &str,
) -> Result<PathBuf, PathValidationError> {
    // implementation...
}
```

### testing strategy
```rust
// comprehensive test coverage
// integration tests for full functionality
// unit tests for individual components
// property-based testing for security functions

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;
    use tokio_test;
    
    #[tokio::test]
    async fn test_file_size_formatting() {
        assert_eq!(format_file_size(0), "0 B");
        assert_eq!(format_file_size(1024), "1.0 KiB");
        assert_eq!(format_file_size(1536), "1.5 KiB");
        assert_eq!(format_file_size(1048576), "1.0 MiB");
    }
    
    #[tokio::test]
    async fn test_path_jailing_security() {
        let temp_dir = TempDir::new().unwrap();
        let base_path = temp_dir.path();
        
        // valid paths should succeed
        assert!(join_path_jailed(base_path, "file.txt").is_ok());
        assert!(join_path_jailed(base_path, "subdir/file.txt").is_ok());
        
        // traversal attempts should fail
        assert!(join_path_jailed(base_path, "../file.txt").is_err());
        assert!(join_path_jailed(base_path, "../../etc/passwd").is_err());
        assert!(join_path_jailed(base_path, "/etc/passwd").is_err());
    }
    
    async fn create_test_directory() -> TempDir {
        let temp_dir = TempDir::new().unwrap();
        
        // create test files
        tokio::fs::write(
            temp_dir.path().join("test.txt"),
            "test content"
        ).await.unwrap();
        
        tokio::fs::create_dir(
            temp_dir.path().join("subdir")
        ).await.unwrap();
        
        temp_dir
    }
}
```

## performance considerations

### memory efficiency
- leverage rust's zero-cost abstractions throughout
- use borrowed data (`&str`, `&Path`) where possible to minimize allocations
- implement streaming for large file transfers
- efficient string handling with `Cow<str>` for optional allocations
- careful buffer management in upload handling

### async efficiency
- proper async/await usage throughout codebase
- non-blocking file operations with `tokio::fs`
- efficient connection handling with axum's built-in pooling
- backpressure management for uploads and downloads
- streaming responses to handle large files without memory pressure

### binary optimization
```toml
[profile.release]
lto = true              # link-time optimization
codegen-units = 1       # better optimization
panic = "abort"         # smaller binary size
strip = true            # remove debug symbols
```

### caching and optimization
```rust
// efficient asset serving with proper cache headers
pub async fn serve_embedded_asset(path: &str) -> Result<Response, AppError> {
    let asset = StaticAssets::get(path)?;
    
    Ok(Response::builder()
        .header(header::CACHE_CONTROL, "public, max-age=31536000")
        .header(header::ETAG, format!("\"{}\"", asset.etag))
        .body(Body::from(asset.data))?)
}

// streaming file responses for memory efficiency
pub async fn serve_large_file(path: &Path) -> Result<Response, AppError> {
    let file = tokio::fs::File::open(path).await?;
    let stream = ReaderStream::new(file);
    let body = Body::from_stream(stream);
    
    Ok(Response::builder()
        .header(header::CONTENT_TYPE, mime_type)
        .body(body)?)
}
```

## security model

### path security implementation
```rust
// comprehensive path jailing prevents all known traversal attacks
pub fn ensure_path_within_jail(
    jail_root: &Path,
    target_path: &Path,
) -> Result<PathBuf, PathTraversalError> {
    // canonicalize both paths to resolve symlinks and relative components
    let canonical_jail = jail_root.canonicalize()
        .map_err(|_| PathTraversalError::InvalidJailRoot)?;
    
    let canonical_target = target_path.canonicalize()
        .map_err(|_| PathTraversalError::InvalidTargetPath)?;
    
    // ensure target is within jail boundaries
    if !canonical_target.starts_with(&canonical_jail) {
        return Err(PathTraversalError::OutsideJail {
            jail: canonical_jail,
            target: canonical_target,
        });
    }
    
    Ok(canonical_target)
}
```

### authentication security
```rust
// constant-time credential comparison prevents timing attacks
fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    
    let mut result = 0u8;
    for (x, y) in a.iter().zip(b.iter()) {
        result |= x ^ y;
    }
    
    result == 0
}

// secure credential validation
pub fn validate_basic_auth(
    auth_header: &str,
    expected_username: &str,
    expected_password: &str,
) -> Result<(), AuthError> {
    let credentials = parse_basic_auth_header(auth_header)?;
    
    let username_valid = constant_time_eq(
        credentials.username.as_bytes(),
        expected_username.as_bytes(),
    );
    
    let password_valid = constant_time_eq(
        credentials.password.as_bytes(),
        expected_password.as_bytes(),
    );
    
    if username_valid && password_valid {
        Ok(())
    } else {
        Err(AuthError::InvalidCredentials)
    }
}
```

### input validation
```rust
// comprehensive input sanitization
pub fn sanitize_filename(filename: &str) -> Result<String, ValidationError> {
    // reject dangerous characters
    if filename.contains('\0') || filename.contains('/') || filename.contains('\\') {
        return Err(ValidationError::DangerousCharacters);
    }
    
    // reject reserved names
    if matches!(filename, "." | ".." | "CON" | "PRN" | "AUX" | "NUL") {
        return Err(ValidationError::ReservedName);
    }
    
    // limit length
    if filename.len() > MAX_FILENAME_LENGTH {
        return Err(ValidationError::TooLong);
    }
    
    Ok(filename.to_string())
}
```

## deployment strategy

### docker optimization
```dockerfile
# multi-stage build for minimal production image
FROM rust:1.70-alpine as builder

# install build dependencies
RUN apk add --no-cache musl-dev

WORKDIR /app
COPY Cargo.toml Cargo.lock ./
COPY src ./src
COPY assets ./assets

# build optimized binary
RUN cargo build --release --target x86_64-unknown-linux-musl

# minimal runtime image
FROM alpine:latest

# install ca-certificates for https
RUN apk add --no-cache ca-certificates

# create non-root user
RUN adduser -D -s /bin/sh soop

# copy binary
COPY --from=builder /app/target/x86_64-unknown-linux-musl/release/soop3 /usr/local/bin/

USER soop
EXPOSE 8000

ENTRYPOINT ["soop3"]
```

### performance benchmarking
```rust
// comprehensive benchmarks to ensure performance parity with d version
use criterion::{black_box, criterion_group, criterion_main, Criterion};

fn bench_file_serving(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().unwrap();
    
    c.bench_function("serve_1mb_file", |b| {
        b.iter(|| {
            rt.block_on(async {
                black_box(serve_test_file(TEST_1MB_FILE).await)
            })
        })
    });
}

fn bench_directory_listing(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().unwrap();
    
    c.bench_function("list_1000_files", |b| {
        b.iter(|| {
            rt.block_on(async {
                black_box(generate_directory_listing(TEST_1000_FILES_DIR).await)
            })
        })
    });
}

criterion_group!(benches, bench_file_serving, bench_directory_listing);
criterion_main!(benches);
```

### continuous integration
```yaml
# .github/workflows/ci.yml
name: CI

on: [push, pull_request]

jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v3
      - uses: actions-rs/toolchain@v1
        with:
          toolchain: stable
          
      - name: Run tests
        run: cargo test --all-features
        
      - name: Run clippy
        run: cargo clippy -- -D warnings
        
      - name: Check formatting
        run: cargo fmt -- --check
        
      - name: Security audit
        run: cargo audit
        
      - name: Run benchmarks
        run: cargo bench
        
  docker:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v3
      - name: Build docker image
        run: docker build -t soop3:latest .
        
      - name: Test docker image
        run: docker run --rm soop3:latest --version
```

## success criteria

### functionality parity
- [ ] identical cli interface to soop2
- [ ] identical toml configuration format
- [ ] identical web ui and directory listing
- [ ] identical security features (path jailing, auth)
- [ ] identical upload functionality
- [ ] identical embedded asset serving

### performance targets
- [ ] memory usage ≤ soop2 d version
- [ ] throughput ≥ soop2 d version  
- [ ] latency ≤ soop2 d version
- [ ] binary size ≤ 150% of optimized d version
- [ ] startup time ≤ soop2 d version

### code quality
- [ ] 100% test coverage for security-critical functions
- [ ] ≥ 90% overall test coverage
- [ ] zero clippy warnings in strict mode
- [ ] comprehensive documentation
- [ ] security audit passes

### deployment
- [ ] docker image builds successfully
- [ ] static binary for linux deployment
- [ ] cross-compilation support
- [ ] production-ready configuration

this comprehensive plan ensures we maintain all the excellent qualities of soop2 while leveraging rust's advantages for improved safety, performance, and maintainability.