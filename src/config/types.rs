// configuration type definitions

use clap::Parser;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// command line interface definition
#[derive(Parser, Debug, Clone)]
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

    /// increase verbosity (-v, -vv, -vvv)
    #[arg(short = 'v', long, action = clap::ArgAction::Count)]
    pub verbose: u8,

    /// decrease verbosity (-q, -qq)
    #[arg(short = 'q', long, action = clap::ArgAction::Count)]
    pub quiet: u8,
}

/// complete application configuration
#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct AppConfig {
    pub server: ServerConfig,
    pub security: SecurityConfig,
    pub listing: ListingConfig,
    pub upload: UploadConfig,
}

/// server configuration section
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ServerConfig {
    pub host: String,
    pub port: u16,
    pub public_dir: PathBuf,
    pub upload_dir: Option<PathBuf>,
    pub enable_upload: bool,
}

/// security and authentication configuration
#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct SecurityConfig {
    pub username: Option<String>,
    pub password: Option<String>,
    #[serde(default)]
    pub policy: SecurityPolicy,
}

/// directory listing configuration
#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct ListingConfig {
    pub ignore_file: Option<PathBuf>,
}

/// file upload configuration
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct UploadConfig {
    #[serde(default = "default_max_request_size")]
    pub max_request_size: u64,
    #[serde(default = "default_true")]
    pub prepend_timestamp: bool,
    #[serde(default = "default_true")]
    pub prevent_overwrite: bool,
    #[serde(default)]
    pub create_directories: bool,
}

/// authentication policy options
#[derive(Debug, Clone, Copy, Deserialize, Serialize, Default, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SecurityPolicy {
    #[default]
    AuthenticateNone,
    AuthenticateAll,
    AuthenticateUpload,
    AuthenticateDownload,
}

impl std::str::FromStr for SecurityPolicy {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "authenticate_none" => Ok(SecurityPolicy::AuthenticateNone),
            "authenticate_upload" => Ok(SecurityPolicy::AuthenticateUpload),
            "authenticate_download" => Ok(SecurityPolicy::AuthenticateDownload),
            "authenticate_all" => Ok(SecurityPolicy::AuthenticateAll),
            _ => Err(format!("Invalid security policy: {s}")),
        }
    }
}

impl AppConfig {
    /// get the effective upload directory (defaults to public_dir if not set)
    pub fn upload_dir(&self) -> &PathBuf {
        self.server
            .upload_dir
            .as_ref()
            .unwrap_or(&self.server.public_dir)
    }
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            host: "0.0.0.0".to_string(),
            port: 8000,
            public_dir: PathBuf::from("."),
            upload_dir: None,
            enable_upload: false,
        }
    }
}

impl Default for UploadConfig {
    fn default() -> Self {
        Self {
            max_request_size: default_max_request_size(),
            prepend_timestamp: default_true(),
            prevent_overwrite: default_true(),
            create_directories: false,
        }
    }
}

// default value functions for serde
fn default_max_request_size() -> u64 {
    1024 * 1024 * 1024 // 1 GiB
}

fn default_true() -> bool {
    true
}
