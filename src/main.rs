mod config;

#[tokio::main]
async fn main() {
    let multicast = config::MultiCast::new().await;
    multicast.presence().await;
    multicast.listen().await;
}
