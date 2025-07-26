mod application;
mod config;
mod domain;
mod infra;
mod proto;
mod utils;

#[tokio::main]
async fn main() -> tokio::io::Result<()> {
    let config = config::init();
    let mut synchronizer = crate::application::Synchronizer::new_default(config).await;

    tokio::select! {
        result = synchronizer.run() => {
            result?;
        },

        _ = tokio::signal::ctrl_c() => {
            synchronizer.shutdown();
            tracing::info!("✅ Synche sucessfully shutdown");
        }
    };
    Ok(())
}
