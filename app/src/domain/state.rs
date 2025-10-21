use crate::domain::{CanonicalPath, Config, ConfigPorts};
use std::{net::IpAddr, sync::Arc};
use tokio::sync::RwLock;
use uuid::Uuid;

pub struct AppState {
    pub ports: ConfigPorts,
    pub local_id: Uuid,
    pub hostname: String,
    pub home_path: CanonicalPath,
    local_ip: RwLock<IpAddr>,
}

impl AppState {
    pub fn new(config: &Config) -> Arc<Self> {
        let hostname = hostname::get().unwrap().to_string_lossy().to_string();
        let local_ip = local_ip_address::local_ip().unwrap();

        Arc::new(Self {
            hostname,
            ports: config.ports.clone(),
            local_id: config.device_id,
            home_path: config.home_path.clone(),
            local_ip: RwLock::new(local_ip),
        })
    }

    pub async fn local_ip(&self) -> IpAddr {
        *self.local_ip.read().await
    }
}
