use std::{net::Ipv4Addr, sync::Arc, time::Duration};
use tokio::{net::UdpSocket, time::sleep};

const MULTICAST_ADDR: &'static str = "239.0.0.1:9999";

pub struct MultiCast {
    socket: Arc<UdpSocket>,
}

impl MultiCast {
    pub async fn new() -> Self {
        let socket = Arc::new(UdpSocket::bind("0.0.0.0:9999").await.unwrap());
        socket.set_multicast_loop_v4(false).unwrap();
        socket
            .join_multicast_v4(Ipv4Addr::new(239, 0, 0, 1), Ipv4Addr::new(0, 0, 0, 0))
            .unwrap();

        Self { socket }
    }

    pub async fn listen(&self) {
        let socket = self.socket.clone();
        tokio::spawn(async move {
            let mut buf = [0; 1024];
            loop {
                match socket.recv_from(&mut buf).await {
                    Ok((len, src)) => {
                        let msg = String::from_utf8_lossy(&buf[..len]);
                        println!("Multicast from {}: {}", src, msg);
                        socket.send_to(b"pong", src).await.unwrap();
                    }
                    Err(e) => eprintln!("Receive error: {}", e),
                }
            }
        });
    }

    pub async fn presence(&self) {
        let socket = self.socket.clone();
        tokio::spawn(async move {
            loop {
                if let Err(err) = socket.send_to(b"ping", MULTICAST_ADDR).await {
                    eprintln!("Sending Presence error: {}", err);
                }
                println!("Sent Presence");
                sleep(Duration::from_secs(5)).await;
            }
        })
        .await
        .unwrap();
    }
}
