mod application;
mod cfg;
mod domain;
mod infra;
mod proto;
mod utils;

#[tokio::main]
async fn main() -> tokio::io::Result<()> {
    let config = cfg::new_default();

    let state = cfg::AppState::new_default(config).await;
    let mut synchronizer = application::Synchronizer::new_default(state).await;

    synchronizer.run().await
}

#[cfg(test)]
mod tests {
    pub mod sqlite;
}
