mod application;
mod config;
mod domain;
mod infra;
mod proto;
mod utils;

use tokio::signal;

#[tokio::main]
async fn main() -> tokio::io::Result<()> {
    let config = config::init();
    let mut synchronizer = application::Synchronizer::new_default(config).await;

    #[cfg(unix)]
    {
        use tokio::signal::unix::{SignalKind, signal};
        let ctrl_c = signal::ctrl_c();
        let mut sigterm = signal(SignalKind::terminate()).expect("bind SIGTERM");
        let mut sighup = signal(SignalKind::hangup()).expect("bind SIGHUP");

        tokio::select! {
            res = synchronizer.run() => res?,

            _ = ctrl_c => {
                tracing::info!("🛑 SIGINT"); synchronizer.shutdown().await?;
            }

            _ = sigterm.recv() => {
                tracing::info!("🛑 SIGTERM"); synchronizer.shutdown().await?;
            }

            _ = sighup.recv() => {
                tracing::info!("🛑 SIGHUP"); synchronizer.shutdown().await?;
            }
        }
    }

    #[cfg(not(unix))]
    {
        let ctrl_c = signal::ctrl_c();

        tokio::select! {
            res = synchronizer.run() => res?,

            _ = ctrl_c => {
                tracing::info!("🛑 SIGINT"); synchronizer.shutdown().await?;
            }
        }
    }

    Ok(())
}
