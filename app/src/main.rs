mod application;
mod cfg;
mod configv1;
mod domain;
mod infra;
mod proto;
mod utils;

#[tokio::main]
async fn main() -> tokio::io::Result<()> {
    let config = configv1::init();

    let mut synchronizer = application::Synchronizer::new_default(config).await;

    tokio::select! {
        res = application::http::server::run() => {res?},
        res = synchronizer.run() => {res?}
    };
    Ok(())
}
