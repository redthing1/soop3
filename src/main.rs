// soop3: the based http fileserver (rust port)
// main entry point with minimal bootstrap logic

use anyhow::Result;
use clap::Parser;
use tracing::{info, Level};

mod config;
mod server;
mod utils;

use config::{load_configuration, Cli};
use server::start_server;

#[tokio::main]
async fn main() -> Result<()> {
    // parse command line arguments
    let cli = Cli::parse();

    // initialize logging based on verbosity flags
    init_logging(cli.verbose, cli.quiet)?;

    // load and merge configuration from file and cli
    let config = load_configuration(&cli)?;

    // start the http server
    start_server(config).await
}

/// initialize structured logging with tracing
fn init_logging(verbose_count: u8, quiet_count: u8) -> Result<()> {
    // calculate log level: info (default) + verbose - quiet
    let base_level = 2i8; // info level
    let adjustment = verbose_count as i8 - quiet_count as i8;
    let final_level = (base_level + adjustment).clamp(0, 4);

    let level = match final_level {
        i8::MIN..=0 => Level::ERROR,
        1 => Level::WARN,
        2 => Level::INFO,
        3 => Level::DEBUG,
        4.. => Level::TRACE,
    };

    tracing_subscriber::fmt()
        .with_max_level(level)
        .with_target(false)
        .with_thread_ids(false)
        .with_file(false)
        .with_line_number(false)
        .compact()
        .init();

    info!("logging initialized at level: {}", level);
    Ok(())
}
