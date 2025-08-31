// configuration loading and merging logic

use anyhow::{Context, Result};
use figment::{
    Figment,
    providers::{Format, Serialized, Toml},
};
use tracing::{debug, info};

use super::types::{AppConfig, Cli, ServerConfig};

/// load and merge configuration from multiple sources
/// precedence: defaults < config file < cli arguments
pub fn load_configuration(cli: &Cli) -> Result<AppConfig> {
    debug!("loading configuration with cli args: {:?}", cli);

    // start with default configuration
    let mut figment = Figment::new().merge(Serialized::defaults(AppConfig::default()));

    // merge config file if provided
    if let Some(config_path) = &cli.config_file {
        if config_path.exists() {
            info!("loading config file: {}", config_path.display());
            figment = figment.merge(Toml::file(config_path));
        } else {
            anyhow::bail!("config file not found: {}", config_path.display());
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
fn cli_overrides(cli: &Cli) -> Serialized<ServerConfig> {
    let server_overrides = ServerConfig {
        host: cli.host.clone(),
        port: cli.port,
        public_dir: cli.public_dir.clone(),
        upload_dir: None, // cli doesn't override upload_dir
        enable_upload: cli.enable_upload,
        cors_origins: cli.cors.clone(),
    };

    Serialized::defaults(server_overrides).key("server")
}

/// validate configuration for consistency and security
fn validate_configuration(config: &AppConfig) -> Result<()> {
    // validate public directory exists
    if !config.server.public_dir.exists() {
        anyhow::bail!(
            "public directory does not exist: {}",
            config.server.public_dir.display()
        );
    }

    if !config.server.public_dir.is_dir() {
        anyhow::bail!(
            "public directory is not a directory: {}",
            config.server.public_dir.display()
        );
    }

    // validate upload directory if uploads are enabled
    if config.server.enable_upload {
        let upload_dir = config.upload_dir();
        if !upload_dir.exists() && !config.upload.create_directories {
            anyhow::bail!(
                "upload directory does not exist and create_directories is false: {}",
                upload_dir.display()
            );
        }
    }

    // validate authentication configuration
    if let (Some(_), None) | (None, Some(_)) =
        (&config.security.username, &config.security.password)
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
    use std::{fs, path::PathBuf};
    use tempfile::TempDir;

    #[test]
    fn test_default_configuration() {
        let cli = Cli {
            public_dir: PathBuf::from("."),
            enable_upload: false,
            host: "127.0.0.1".to_string(),
            port: 8080,
            config_file: None,
            verbose: 0,
            quiet: 0,
            cors: vec![],
        };

        let config = load_configuration(&cli).unwrap();

        assert_eq!(config.server.host, "127.0.0.1");
        assert_eq!(config.server.port, 8080);
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
            public_dir: temp_dir.path().to_path_buf(),
            enable_upload: false,          // should be overridden by config file
            host: "127.0.0.1".to_string(), // should be overridden by config file
            port: 8080,                    // should be overridden by config file
            config_file: Some(config_path),
            verbose: 0,
            quiet: 0,
            cors: vec![],
        };

        let config = load_configuration(&cli).unwrap();

        // config file should override defaults but cli should override config file
        assert_eq!(config.server.host, "127.0.0.1"); // cli override
        assert_eq!(config.server.port, 8080); // cli override
        assert!(!config.server.enable_upload); // cli override
        assert!(!config.upload.prepend_timestamp); // from config file
    }
}
