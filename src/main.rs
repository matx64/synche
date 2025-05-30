use local_ip_address::local_ip;
use std::{
    collections::HashSet,
    net::SocketAddr,
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio::{io, net::UdpSocket};

const BROADCAST_PORT: u16 = 8888;
const BROADCAST_INTERVAL_SECS: u64 = 5;

#[tokio::main]
async fn main() -> io::Result<()> {
    let bind_addr = format!("0.0.0.0:{}", BROADCAST_PORT);

    let socket = Arc::new(UdpSocket::bind(&bind_addr).await?);
    socket.set_broadcast(true)?;
    let devices = Arc::new(Mutex::new(HashSet::<SocketAddr>::new()));

    let send_task = tokio::spawn(send(socket.clone()));
    let recv_task = tokio::spawn(recv(socket, devices.clone()));
    let state_task = tokio::spawn(state(devices));

    tokio::select! {
        _ = send_task => {},
        _ = recv_task => {},
        _ = state_task => {},
    };
    Ok(())
}

async fn send(socket: Arc<UdpSocket>) -> io::Result<()> {
    let broadcast_addr = format!("255.255.255.255:{}", BROADCAST_PORT);

    loop {
        socket.send_to("ping".as_bytes(), &broadcast_addr).await?;
        tokio::time::sleep(Duration::from_secs(BROADCAST_INTERVAL_SECS)).await;
    }
}

async fn recv(socket: Arc<UdpSocket>, devices: Arc<Mutex<HashSet<SocketAddr>>>) -> io::Result<()> {
    let local_ip = SocketAddr::new(local_ip().unwrap(), BROADCAST_PORT);
    let mut buf = [0; 1024];

    loop {
        let (size, src_addr) = socket.recv_from(&mut buf).await?;

        let _msg = String::from_utf8_lossy(&buf[..size]);

        let mut devices = devices.lock().unwrap();
        if src_addr != local_ip && !devices.contains(&src_addr) {
            devices.insert(src_addr);
            println!("Device connected: {}", src_addr);
        }
    }
}

async fn state(devices: Arc<Mutex<HashSet<SocketAddr>>>) {
    println!(
        "🚀 Synche running on port {}. Press Ctrl+C to stop.",
        BROADCAST_PORT
    );
    loop {
        tokio::time::sleep(Duration::from_secs(60)).await;
        let devices = devices.lock().unwrap();
        println!("Connected Synche devices: {:?}", devices);
    }
}
