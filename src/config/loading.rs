// configuration loading and merging logic

use anyhow::{Context, Result};
use figment::{
    Figment,
    providers::{Format, Serialized, Toml},
};
use serde::Serialize;
use tracing::{debug, info};

use super::types::{AppConfig, Cli, SecurityPolicy};
use std::path::PathBuf;
use std::{fs, io::ErrorKind};

/// load and merge configuration from multiple sources
/// precedence: defaults < config file < cli arguments
pub fn load_configuration(cli: &Cli) -> Result<AppConfig> {
    debug!("loading configuration with cli args: {:?}", cli);

    // start with default configuration
    let mut figment = Figment::new().merge(Serialized::defaults(AppConfig::default()));

    // merge config file if provided
    if let Some(config_path) = &cli.config_file {
        match fs::metadata(config_path) {
            Ok(metadata) => {
                if !metadata.is_file() {
                    anyhow::bail!(
                        "config file is not a regular file: {}",
                        config_path.display()
                    );
                }
                info!("loading config file: {}", config_path.display());
                figment = figment.merge(Toml::file(config_path));
            }
            Err(err) if err.kind() == ErrorKind::NotFound => {
                anyhow::bail!("config file not found: {}", config_path.display());
            }
            Err(err) => {
                anyhow::bail!(
                    "failed to read config file metadata: {} ({})",
                    config_path.display(),
                    err
                );
            }
        }
    }

    // merge cli overrides - highest precedence
    figment = figment.merge(cli_overrides(cli));

    // extract final configuration
    let config: AppConfig = figment.extract().context("failed to parse configuration")?;

    // validate configuration
    validate_configuration(&config)?;

    debug!("final configuration: {:?}", config);
    Ok(config)
}

/// convert cli arguments to configuration overrides
#[derive(Serialize)]
struct ServerConfigOverrides {
    #[serde(skip_serializing_if = "Option::is_none")]
    host: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    port: Option<u16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    public_dir: Option<PathBuf>,
    #[serde(skip_serializing_if = "Option::is_none")]
    enable_upload: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    cors_origins: Option<Vec<String>>,
}

fn cli_overrides(cli: &Cli) -> Serialized<ServerConfigOverrides> {
    let server_overrides = ServerConfigOverrides {
        host: cli.host.clone(),
        port: cli.port,
        public_dir: cli.public_dir.clone(),
        enable_upload: if cli.enable_upload { Some(true) } else { None },
        cors_origins: if cli.cors.is_empty() {
            None
        } else {
            Some(cli.cors.clone())
        },
    };

    Serialized::defaults(server_overrides).key("server")
}

/// validate configuration for consistency and security
fn validate_configuration(config: &AppConfig) -> Result<()> {
    // validate public directory exists
    match fs::metadata(&config.server.public_dir) {
        Ok(metadata) => {
            if !metadata.is_dir() {
                anyhow::bail!(
                    "public directory is not a directory: {}",
                    config.server.public_dir.display()
                );
            }
        }
        Err(err) if err.kind() == ErrorKind::NotFound => {
            anyhow::bail!(
                "public directory does not exist: {}",
                config.server.public_dir.display()
            );
        }
        Err(err) => {
            anyhow::bail!(
                "failed to read public directory metadata: {} ({})",
                config.server.public_dir.display(),
                err
            );
        }
    }

    // validate upload directory if uploads are enabled
    if config.server.enable_upload {
        let upload_dir = config.upload_dir();
        match fs::metadata(upload_dir) {
            Ok(metadata) => {
                if !metadata.is_dir() {
                    anyhow::bail!(
                        "upload directory is not a directory: {}",
                        upload_dir.display()
                    );
                }
            }
            Err(err) if err.kind() == ErrorKind::NotFound => {
                if !config.upload.create_directories {
                    anyhow::bail!(
                        "upload directory does not exist and create_directories is false: {}",
                        upload_dir.display()
                    );
                }
            }
            Err(err) => {
                anyhow::bail!(
                    "failed to read upload directory metadata: {} ({})",
                    upload_dir.display(),
                    err
                );
            }
        }
    }

    // validate authentication configuration
    if config.security.policy != SecurityPolicy::AuthenticateNone
        && matches!(
            (&config.security.username, &config.security.password),
            (Some(_), None) | (None, Some(_))
        )
    {
        anyhow::bail!("both username and password must be provided for authentication");
    }

    // validate port range
    if config.server.port == 0 {
        anyhow::bail!("port cannot be 0");
    }

    Ok(())
}

/// load configuration from a file for testing purposes
#[cfg(feature = "test-helpers")]
#[allow(dead_code)]
pub fn load_config_from_file(config_path: &std::path::Path) -> Result<AppConfig> {
    let figment = Figment::new()
        .merge(Serialized::defaults(AppConfig::default()))
        .merge(Toml::file(config_path));

    let config: AppConfig = figment
        .extract()
        .context("failed to parse configuration file")?;

    Ok(config)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn test_default_configuration() {
        let cli = Cli {
            public_dir: None,
            enable_upload: false,
            host: None,
            port: None,
            config_file: None,
            verbose: 0,
            quiet: 0,
            cors: vec![],
        };

        let config = load_configuration(&cli).unwrap();

        assert_eq!(config.server.host, "0.0.0.0");
        assert_eq!(config.server.port, 8000);
        assert!(!config.server.enable_upload);
    }

    #[test]
    fn test_config_file_loading() {
        let temp_dir = TempDir::new().unwrap();
        let config_path = temp_dir.path().join("config.toml");

        fs::write(
            &config_path,
            r#"
[server]
host = "192.168.1.1"
port = 9000
enable_upload = true

[upload]
prepend_timestamp = false
"#,
        )
        .unwrap();

        let cli = Cli {
            public_dir: None,
            enable_upload: false,
            host: None,
            port: None,
            config_file: Some(config_path),
            verbose: 0,
            quiet: 0,
            cors: vec![],
        };

        let config = load_configuration(&cli).unwrap();

        // config file should override defaults when no cli flags are set
        assert_eq!(config.server.host, "192.168.1.1");
        assert_eq!(config.server.port, 9000);
        assert!(config.server.enable_upload);
        assert!(!config.upload.prepend_timestamp); // from config file
    }

    #[test]
    fn test_config_file_must_be_regular_file() {
        let temp_dir = TempDir::new().unwrap();
        let config_dir = temp_dir.path().join("config_dir");
        fs::create_dir(&config_dir).unwrap();

        let cli = Cli {
            public_dir: None,
            enable_upload: false,
            host: None,
            port: None,
            config_file: Some(config_dir),
            verbose: 0,
            quiet: 0,
            cors: vec![],
        };

        let err = load_configuration(&cli).unwrap_err();
        assert!(
            err.to_string()
                .contains("config file is not a regular file"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn test_missing_config_file_returns_error() {
        let temp_dir = TempDir::new().unwrap();
        let config_path = temp_dir.path().join("missing.toml");

        let cli = Cli {
            public_dir: None,
            enable_upload: false,
            host: None,
            port: None,
            config_file: Some(config_path),
            verbose: 0,
            quiet: 0,
            cors: vec![],
        };

        let err = load_configuration(&cli).unwrap_err();
        assert!(
            err.to_string().contains("config file not found"),
            "unexpected error: {err}"
        );
    }
}
