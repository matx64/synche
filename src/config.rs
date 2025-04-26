use std::{net::Ipv4Addr, sync::Arc, time::Duration};
use tokio::{net::UdpSocket, time::sleep};

const MULTICAST_ADDR: &str = "239.0.0.1:9999";

pub struct MultiCast {
    socket: Arc<UdpSocket>,
}

impl MultiCast {
    pub async fn new() -> Self {
        let socket = UdpSocket::bind("0.0.0.0:9999").await.unwrap();
        socket.set_multicast_loop_v4(false).unwrap();
        socket
            .join_multicast_v4(Ipv4Addr::new(239, 0, 0, 1), Ipv4Addr::UNSPECIFIED)
            .unwrap();

        Self {
            socket: Arc::new(socket),
        }
    }

    pub async fn listen(&self) {
        let socket = self.socket.clone();
        tokio::spawn(async move {
            let mut buf = [0u8; 1024];
            loop {
                match socket.recv_from(&mut buf).await {
                    Ok((len, src)) => {
                        let msg = String::from_utf8_lossy(&buf[..len]);
                        println!("Received multicast from {}: {}", src, msg);

                        if let Err(e) = socket.send_to(b"pong", src).await {
                            eprintln!("Failed to send pong to {}: {}", src, e);
                        }
                    }
                    Err(e) => {
                        eprintln!("Error receiving multicast: {}", e);
                        break;
                    }
                }
            }
        })
        .await
        .unwrap();
    }

    pub async fn presence(&self) {
        let socket = self.socket.clone();
        tokio::spawn(async move {
            loop {
                match socket.send_to(b"ping", MULTICAST_ADDR).await {
                    Ok(_) => println!("Presence ping sent."),
                    Err(e) => eprintln!("Error sending presence: {}", e),
                }
                sleep(Duration::from_secs(5)).await;
            }
        })
        .await
        .unwrap();
    }
}
