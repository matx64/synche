mod application;
mod domain;
mod infra;
mod utils;

#[tokio::main]
async fn main() -> tokio::io::Result<()> {
    tracing_subscriber::fmt::init();

    let mut synchronizer = application::Synchronizer::new_default().await;

    synchronizer.run().await
}

#[cfg(test)]
mod tests {
    pub mod sqlite;
}
