use crate::{
    application::AppState,
    application::network::presence::interface::{PresenceEvent, PresenceInterface},
};
use mdns_sd::{
    IfKind, Receiver, ResolvedService, ServiceDaemon, ServiceEvent, ServiceInfo, TxtProperties,
};
use std::sync::Arc;
use tokio::io;
use tracing::{error, info, warn};
use uuid::Uuid;

const SERVICE_TYPE: &str = "_synche._udp.local.";
const RETRY_COUNT: usize = 3;

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

        let service_type = SERVICE_TYPE.to_string();
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
        let hostname = self.state.hostname().clone() + ".local.";
        let properties = [("instance_id", self.state.instance_id())];

        let service_info = ServiceInfo::new(
            &self.service_type,
            &self.state.local_id().to_string(),
            &hostname,
            self.state.local_ip().await,
            self.state.ports().presence,
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
        if id == self.state.local_id() {
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
        let fullname = format!("{}.{}", self.state.local_id(), self.service_type);
        Self::retry_mdns_operation("mDNS UNREGISTER", || self.daemon.unregister(&fullname));
    }

    fn shutdown_daemon(&self) {
        Self::retry_mdns_operation("mDNS SHUTDOWN", || self.daemon.shutdown());
    }

    fn retry_mdns_operation<T>(
        operation_name: &str,
        operation_fn: impl Fn() -> Result<Receiver<T>, mdns_sd::Error>,
    ) where
        T: std::fmt::Debug,
    {
        for _ in 0..RETRY_COUNT {
            match operation_fn() {
                Err(mdns_sd::Error::Again) => continue,
                Ok(recv) => {
                    if let Ok(res) = recv.recv() {
                        info!("{} Status: {:?}", operation_name, res);
                    }
                    return;
                }
                _ => return,
            }
        }
        error!(
            "Failed to {} after {} attempts",
            operation_name, RETRY_COUNT
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    async fn create_test_adapter() -> MdnsAdapter {
        MdnsAdapter::new(AppState::new().await)
    }

    #[tokio::test]
    async fn test_get_peer_id_valid_uuid() {
        let adapter = create_test_adapter().await;

        let uuid = Uuid::new_v4();
        let fullname = format!("{}._synche._udp.local.", uuid);

        let result = adapter.get_peer_id(&fullname);
        assert_eq!(result, Some(uuid));
    }

    #[tokio::test]
    async fn test_get_peer_id_invalid_uuid() {
        let adapter = create_test_adapter().await;

        let fullname = "not-a-uuid._synche._udp.local.";
        let result = adapter.get_peer_id(fullname);

        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_get_peer_id_empty_string() {
        let adapter = create_test_adapter().await;

        let fullname = "";
        let result = adapter.get_peer_id(fullname);

        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_handle_service_removed_valid() {
        let adapter = create_test_adapter().await;

        let uuid = Uuid::new_v4();
        let fullname = format!("{}._synche._udp.local.", uuid);
        let result = adapter.handle_service_removed(&fullname);

        assert!(result.is_some());
        match result.unwrap() {
            PresenceEvent::Disconnect(id) => assert_eq!(id, uuid),
            _ => panic!("Expected Disconnect event"),
        }
    }

    #[tokio::test]
    async fn test_handle_service_removed_invalid_fullname() {
        let adapter = create_test_adapter().await;

        let fullname = "invalid-uuid._synche._udp.local.";
        let result = adapter.handle_service_removed(fullname);

        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_handle_service_removed_empty_fullname() {
        let adapter = create_test_adapter().await;

        let fullname = "";
        let result = adapter.handle_service_removed(fullname);

        assert!(result.is_none());
    }
}
