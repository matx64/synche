use crate::{
    application::network::presence::interface::{PresenceEvent, PresenceInterface},
    domain::AppState,
};
use mdns_sd::{
    IfKind, Receiver, ResolvedService, ServiceDaemon, ServiceEvent, ServiceInfo, TxtProperties,
};
use std::sync::Arc;
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
        let properties = [("instance_id", self.state.instance_id)];

        let service_info = ServiceInfo::new(
            &self.service_type,
            &self.state.local_id.to_string(),
            &hostname,
            self.state.local_ip().await,
            self.state.ports.presence,
            &properties[..],
        )
        .map_err(io::Error::other)?
        .enable_addr_auto();

        self.daemon.register(service_info).map_err(io::Error::other)
    }

    async fn next(&self) -> io::Result<Option<PresenceEvent>> {
        loop {
            match self.receiver.recv_async().await.map_err(io::Error::other)? {
                ServiceEvent::ServiceResolved(info) => {
                    if let Some(event) = self.handle_service_resolved(*info) {
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
        self.unregister();
        self.shutdown_daemon();
    }
}

impl MdnsAdapter {
    fn handle_service_resolved(&self, info: ResolvedService) -> Option<PresenceEvent> {
        let id = self.get_peer_id(&info.fullname)?;
        if id == self.state.local_id {
            return None;
        }

        let instance_id = self.get_peer_instance_id(info.get_properties())?;

        for addr in info.addresses {
            if addr.is_ipv6() {
                continue;
            }

            let addr = addr.to_ip_addr();
            if addr.is_loopback() {
                continue;
            }

            return Some(PresenceEvent::Ping {
                id,
                addr,
                instance_id,
            });
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

    fn get_peer_instance_id(&self, props: &TxtProperties) -> Option<Uuid> {
        let instance_bytes = props.get_property_val("instance_id")??;

        let instance_str = std::str::from_utf8(instance_bytes).ok()?;
        Uuid::parse_str(instance_str).ok()
    }

    fn unregister(&self) {
        for _ in 0..3 {
            match self
                .daemon
                .unregister(&format!("{}.{}", self.state.local_id, self.service_type))
            {
                Err(mdns_sd::Error::Again) => continue,

                Ok(recv) => {
                    if let Ok(res) = recv.recv() {
                        info!("mDNS UNREGISTER Status: {res:?}");
                    }
                    return;
                }
                _ => return,
            }
        }
        error!("Failed to unregister mDNS daemon after 3 attempts");
    }

    fn shutdown_daemon(&self) {
        for _ in 0..3 {
            match self.daemon.shutdown() {
                Err(mdns_sd::Error::Again) => continue,

                Ok(recv) => {
                    if let Ok(res) = recv.recv() {
                        info!("mDNS SHUTDOWN Status: {res:?}");
                    }
                    return;
                }
                _ => {
                    return;
                }
            }
        }
        error!("Failed to shutdown mDNS daemon after 3 attempts");
    }
}
