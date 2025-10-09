mod application;
mod cfg;
mod domain;
mod infra;
mod proto;
mod utils;

#[tokio::main]
async fn main() -> tokio::io::Result<()> {
    let config = cfg::new_default();

    let state = cfg::AppState::new_default(config);
    let mut synchronizer = application::Synchronizer::new_default(state).await;

    tokio::select! {
        res = application::http::server::run() => {res?},
        res = synchronizer.run() => {res?}
    };
    Ok(())
}
