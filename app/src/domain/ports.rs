use serde::{Deserialize, Serialize};

/// The three network ports the application binds.
///
/// `http` serves the GUI and JSON API; `presence` is the mDNS discovery
/// port; `transport` is the TCP port that carries handshakes, metadata,
/// requests, and entry transfers.
///
/// A value of `0` requests an OS-assigned ephemeral port — tests rely on
/// this so they don't collide with the production defaults defined in
/// `app_state::resolve_ports`.
#[derive(Serialize, Deserialize, Clone)]
pub struct AppPorts {
    pub http: u16,
    pub presence: u16,
    pub transport: u16,
}

/// Optional per-port overrides.
///
/// Used both for the CLI flags (`--http-port`/`--presence-port`/`--transport-port`)
/// and for the optional `[ports]` block in `config.toml`. A `None` field means
/// "not set", so it falls back to the next precedence level. Resolution order is
/// CLI > config > default constant — see `app_state::resolve_ports`.
#[derive(Serialize, Deserialize, Clone, Default)]
pub struct PortOverrides {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub http: Option<u16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub presence: Option<u16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub transport: Option<u16>,
}

impl PortOverrides {
    /// True when no port is set. Used to omit an empty `[ports]` table when
    /// serializing the default `config.toml`.
    pub fn is_empty(&self) -> bool {
        self.http.is_none() && self.presence.is_none() && self.transport.is_none()
    }

    /// All ports set to `0` (OS-assigned ephemeral). Tests use this so parallel
    /// runs don't collide on the production defaults.
    #[cfg(test)]
    pub fn ephemeral() -> Self {
        Self {
            http: Some(0),
            presence: Some(0),
            transport: Some(0),
        }
    }
}
