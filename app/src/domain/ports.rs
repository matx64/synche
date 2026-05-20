use serde::{Deserialize, Serialize};

/// The three network ports the application binds.
///
/// `http` serves the GUI and JSON API; `presence` is the mDNS discovery
/// port; `transport` is the TCP port that carries handshakes, metadata,
/// requests, and entry transfers.
///
/// A value of `0` requests an OS-assigned ephemeral port — tests rely on
/// this so they don't collide with the production defaults defined in
/// `AppState::default_ports`.
#[derive(Serialize, Deserialize, Clone)]
pub struct AppPorts {
    pub http: u16,
    pub presence: u16,
    pub transport: u16,
}
