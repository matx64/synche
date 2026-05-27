mod application;
mod domain;
mod infra;
mod utils;

#[tokio::main]
async fn main() -> tokio::io::Result<()> {
    let dirs = utils::dirs::SyncheDirs::from_os()?;
    // Must outlive `main`: dropping the guard discards in-flight log lines.
    let _log_guards = utils::logging::init(dirs.log_dir().as_ref());

    tracing::info!("Synche v{}", env!("CARGO_PKG_VERSION"));

    application::Synchronizer::run_default_with_restart(dirs).await
}
