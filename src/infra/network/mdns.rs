use mdns_sd::{Receiver, ServiceDaemon, ServiceEvent, ServiceInfo};
use std::collections::HashMap;
use tokio::io;
use tracing::{info, warn};
use uuid::Uuid;

const MDNS_PORT: u16 = 5200;

pub struct MdnsAdapter {
    daemon: ServiceDaemon,
    local_id: Uuid,
    service_type: String,
    receiver: Receiver<ServiceEvent>,
}

impl MdnsAdapter {
    pub fn new(local_id: Uuid) -> Self {
        let daemon = ServiceDaemon::new().expect("Failed to create mdns daemon");

        let service_type = "_synche._udp.local.".to_string();
        let receiver = daemon.browse(&service_type).expect("Failed to browse");

        Self {
            daemon,
            local_id,
            service_type,
            receiver,
        }
    }

    pub fn advertise(&self) {
        let local_ip = local_ip_address::local_ip().unwrap();
        let hostname = hostname::get().unwrap().to_string_lossy().to_string() + ".local.";

        let service_info = ServiceInfo::new(
            &self.service_type,
            &self.local_id.to_string(),
            &hostname,
            local_ip,
            MDNS_PORT,
            None::<HashMap<String, String>>,
        )
        .unwrap();

        self.daemon
            .register(service_info)
            .expect("Failed to register mdns service");
    }

    pub async fn recv(&self) -> io::Result<ServiceEvent> {
        self.receiver.recv_async().await.map_err(io::Error::other)
    }

    pub fn get_peer_id(&self, fullname: &str) -> Option<Uuid> {
        fullname
            .split('.')
            .next()
            .and_then(|id| Uuid::parse_str(id).ok())
    }

    pub fn shutdown(&self) {
        for _ in 0..3 {
            match self.daemon.shutdown() {
                Err(mdns_sd::Error::Again) => continue,
                _ => {
                    info!("mDNS daemon shutdown");
                    return;
                }
            }
        }
        warn!("Failed to shutdown mDNS daemon after 3 attempts");
    }
}
