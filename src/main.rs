use local_ip_address::list_afinet_netifas;
use std::{
    collections::HashMap,
    net::{IpAddr, SocketAddr},
    sync::{Arc, Mutex},
    time::{Duration, SystemTime},
};
use tokio::{io, net::UdpSocket};

const BROADCAST_PORT: u16 = 8888;
const BROADCAST_INTERVAL_SECS: u64 = 5;
const DEVICE_TIMEOUT_SECS: u64 = 15;

#[derive(Debug)]
struct Device {
    addr: SocketAddr,
    last_seen: SystemTime,
}

impl Device {
    pub fn new(addr: SocketAddr) -> Self {
        Self {
            addr,
            last_seen: SystemTime::now(),
        }
    }
}

#[tokio::main]
async fn main() -> io::Result<()> {
    let bind_addr = format!("0.0.0.0:{}", BROADCAST_PORT);

    let socket = Arc::new(UdpSocket::bind(&bind_addr).await?);
    socket.set_broadcast(true)?;
    let devices = Arc::new(Mutex::new(HashMap::<SocketAddr, Device>::new()));

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

async fn recv(
    socket: Arc<UdpSocket>,
    devices: Arc<Mutex<HashMap<SocketAddr, Device>>>,
) -> io::Result<()> {
    let ifas = list_afinet_netifas().unwrap();
    let mut buf = [0; 1024];

    loop {
        let (size, src_addr) = socket.recv_from(&mut buf).await?;

        let msg = String::from_utf8_lossy(&buf[..size]);
        if is_host(&ifas, src_addr.ip()) || msg != "ping" {
            continue;
        }

        {
            let mut devices = devices.lock().unwrap();
            if devices.insert(src_addr, Device::new(src_addr)).is_none() {
                println!("Device connected: {}", src_addr);
            }
        }
    }
}

async fn state(devices: Arc<Mutex<HashMap<SocketAddr, Device>>>) {
    println!(
        "🚀 Synche running on port {}. Press Ctrl+C to stop.",
        BROADCAST_PORT
    );
    loop {
        tokio::time::sleep(Duration::from_secs(10)).await;

        let mut devices = devices.lock().unwrap();
        devices.retain(|_, device| !matches!(device.last_seen.elapsed(), Ok(elapsed) if elapsed.as_secs() > DEVICE_TIMEOUT_SECS));

        if !devices.is_empty() {
            println!(
                "Connected Synche devices: {:?}",
                devices.keys().collect::<Vec<_>>()
            );
        } else {
            println!("No Synche devices connected.");
        }
    }
}

fn is_host(ifas: &[(String, IpAddr)], addr: IpAddr) -> bool {
    ifas.iter().any(|ifa| ifa.1 == addr)
}
