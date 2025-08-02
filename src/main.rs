mod application;
mod config;
mod domain;
mod infra;
mod proto;
mod utils;

use tokio::signal::{self, unix::SignalKind};

#[tokio::main]
async fn main() -> tokio::io::Result<()> {
    let config = config::init();

    let mut synchronizer = crate::application::Synchronizer::new_default(config).await;

    tokio::select! {
        // Main task
        res = synchronizer.run() => {
            res?;
        }

        // Handle Ctrl+C
        _ = signal::ctrl_c() => {
            tracing::info!("🛑 Received Ctrl+C (SIGINT)");
            synchronizer.shutdown().await?;
            tracing::info!("✅ Synche gracefully shutdown (Ctrl+C)");
            return Ok(());
        }

        // Handle SIGTERM (e.g. from systemd or `kill`)
        _ = async {
            let mut sigterm = signal::unix::signal(SignalKind::terminate()).expect("Failed to install SIGTERM handler");
            sigterm.recv().await;
        } => {
            tracing::info!("🛑 Received SIGTERM");
            synchronizer.shutdown().await?;
            tracing::info!("✅ Synche gracefully shutdown (SIGTERM)");
            return Ok(());
        }

        // Handle SIGHUP (e.g. terminal closed)
        _ = async {
            let mut sighup = signal::unix::signal(SignalKind::hangup()).expect("Failed to install SIGHUP handler");
            sighup.recv().await;
        } => {
            tracing::info!("📴 Terminal closed or SIGHUP received");
            synchronizer.shutdown().await?;
            tracing::info!("✅ Synche gracefully shutdown (SIGHUP)");
            return Ok(());
        }
    }
    Ok(())
}
