use crate::{
    application::network::presence::interface::{PresenceEvent, PresenceInterface},
    domain::AppState,
};
use mdns_sd::{IfKind, Receiver, ResolvedService, ServiceDaemon, ServiceEvent, ServiceInfo};
use std::{collections::HashMap, net::IpAddr, sync::Arc};
use tokio::io;
use tracing::{error, info, warn};
use uuid::Uuid;

pub struct MdnsAdapter {
    state: Arc<AppState>,
    daemon: ServiceDaemon,
    service_type: String,
    receiver: Receiver<ServiceEvent>,
}

impl MdnsAdapter {
    pub fn new(state: Arc<AppState>) -> Self {
        let daemon = ServiceDaemon::new().expect("Failed to create mdns daemon");

        daemon.disable_interface(IfKind::IPv6).unwrap();
        daemon.use_service_data(true).unwrap();

        let service_type = "_synche._udp.local.".to_string();
        let receiver = daemon.browse(&service_type).expect("Failed to browse");

        Self {
            state,
            daemon,
            service_type,
            receiver,
        }
    }
}

impl PresenceInterface for MdnsAdapter {
    async fn advertise(&self) -> io::Result<()> {
        let hostname = self.state.hostname.clone() + ".local.";
        let properties = HashMap::from([("instance_id".to_string(), self.state.instance_id.to_string())]);

        let service_info = ServiceInfo::new(
            &self.service_type,
            &self.state.local_id.to_string(),
            &hostname,
            self.state.local_ip().await,
            self.state.ports.presence,
            properties,
        )
            .map_err(io::Error::other)?;

        self.daemon.register(service_info).map_err(io::Error::other)
    }

    async fn next(&self) -> io::Result<Option<PresenceEvent>> {
        loop {
            match self.receiver.recv_async().await.map_err(io::Error::other)? {
                ServiceEvent::ServiceData(info) => {
                    if let Some(event) = self.handle_service_data(*info) {
                        return Ok(Some(event));
                    }
                }

                ServiceEvent::ServiceRemoved(_, fullname) => {
                    if let Some(event) = self.handle_service_removed(&fullname) {
                        return Ok(Some(event));
                    }
                }

                _ => {}
            }
        }
    }

    async fn shutdown(&self) {
        for _ in 0..3 {
            match self.daemon.shutdown() {
                Err(mdns_sd::Error::Again) => continue,
                _ => {
                    info!("mDNS daemon shutdown");
                    return;
                }
            }
        }
        error!("Failed to shutdown mDNS daemon after 3 attempts");
    }
}

impl MdnsAdapter {
    fn handle_service_data(&self, info: ResolvedService) -> Option<PresenceEvent> {
        let peer_id = self.get_peer_id(&info.fullname)?;

        if peer_id == self.state.local_id {
            return None;
        }

        for peer_ip in info.addresses {
            if peer_ip.is_ipv6() {
                continue;
            }

            let peer_ip = peer_ip.to_ip_addr();
            if peer_ip.is_loopback() {
                continue;
            }

            if let Some(instance_id) = info.txt_properties.get("instance_id") && let Ok(instance_id) = Uuid::parse_str(&instance_id.to_string()) {
                return Some(PresenceEvent::Ping { id: peer_id, ip: peer_ip, instance_id });
            }
        }
        None
    }

    fn handle_service_removed(&self, fullname: &str) -> Option<PresenceEvent> {
        match self.get_peer_id(fullname) {
            Some(id) => Some(PresenceEvent::Disconnect(id)),
            None => {
                warn!(fullname = fullname, "Invalid mDNS peer id");
                None
            }
        }
    }

    fn get_peer_id(&self, fullname: &str) -> Option<Uuid> {
        fullname
            .split('.')
            .next()
            .and_then(|id| Uuid::parse_str(id).ok())
    }
}
