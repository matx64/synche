mod application;
mod domain;
mod infra;
mod proto;
mod utils;

#[tokio::main]
async fn main() -> tokio::io::Result<()> {
    let mut synchronizer = application::Synchronizer::new_default().await;

    synchronizer.run().await
}

#[cfg(test)]
mod tests {
    pub mod sqlite;
}
