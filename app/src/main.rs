mod application;
mod domain;
mod infra;
mod utils;

#[tokio::main]
async fn main() -> tokio::io::Result<()> {
    tracing_subscriber::fmt::init();

    application::Synchronizer::run_default_with_restart().await
}

#[cfg(test)]
mod tests {
    pub mod sqlite;
}
