mod application;
mod cli;
mod domain;
mod infra;
mod utils;

use clap::Parser;

#[tokio::main]
async fn main() -> tokio::io::Result<()> {
    // `--version`/`--help` short-circuit and exit inside `parse`.
    let args = cli::Cli::parse();

    // `--config-dir` relocates all state (config, data, logs); otherwise use the
    // OS-default directories.
    let dirs = match args.config_dir {
        Some(ref path) => utils::dirs::SyncheDirs::rooted_at(path)?,
        None => utils::dirs::SyncheDirs::from_os()?,
    };
    // Must outlive `main`: dropping the guard discards in-flight log lines.
    let _log_guards = utils::logging::init(dirs.log_dir().as_ref());

    tracing::info!("Synche v{}", env!("CARGO_PKG_VERSION"));

    application::Synchronizer::run_default_with_restart(dirs, args.port_overrides()).await
}
