use mdns_sd::{Receiver, ServiceDaemon, ServiceEvent, ServiceInfo};
use std::collections::HashMap;
use tracing::{error, info};
use uuid::Uuid;

pub struct MdnsPresence {
    daemon: ServiceDaemon,
    local_id: Uuid,
    service_type: String,
    receiver: Receiver<ServiceEvent>,
}

impl MdnsPresence {
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

        let service_info = ServiceInfo::new(
            &self.service_type,
            &self.local_id.to_string(),
            &format!("{local_ip}.local."),
            local_ip,
            5353,
            HashMap::new(),
        )
        .unwrap();

        self.daemon
            .register(service_info)
            .expect("Failed to register mdns service");
    }

    pub async fn recv(&self) -> tokio::io::Result<()> {
        loop {
            match self.receiver.recv_async().await {
                Ok(event) => match event {
                    ServiceEvent::SearchStarted(hostname) => {
                        info!("ServiceEvent::SearchStarted = {}", hostname);
                    }
                    ServiceEvent::ServiceFound(service_type, fullname) => {
                        info!("ServiceEvent::ServiceFound = {} {}", service_type, fullname);
                    }
                    ServiceEvent::ServiceResolved(service_info) => {
                        info!("ServiceEvent::ServiceResolved = {:?}", service_info);
                    }
                    ServiceEvent::ServiceRemoved(service_type, fullname) => {
                        info!(
                            "ServiceEvent::ServiceRemoved = {} {}",
                            service_type, fullname
                        );
                    }
                    ServiceEvent::SearchStopped(hostname) => {
                        info!("ServiceEvent::SearchStopped = {}", hostname);
                    }
                },
                Err(err) => {
                    error!("mDNS error: {}", err);
                }
            };
        }
    }
}
