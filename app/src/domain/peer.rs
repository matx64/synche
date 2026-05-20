use crate::domain::{RelativePath, SyncDirectory};
use serde::Serialize;
use std::{collections::HashMap, net::IpAddr, time::SystemTime};
use uuid::Uuid;

/// A remote Synche instance currently visible on the network.
///
/// `id` is the peer's persistent device identifier; `instance_id` is
/// regenerated on every process start, so a change to it signals that
/// the peer restarted even when `id` and `addr` stay the same.
/// `last_seen` is refreshed on every presence announcement and is used
/// to evict peers that have gone silent.
#[derive(Debug, Clone, Serialize)]
pub struct Peer {
    pub id: Uuid,
    pub addr: IpAddr,
    pub hostname: String,
    pub instance_id: Uuid,
    pub last_seen: SystemTime,
    pub sync_dirs: HashMap<RelativePath, SyncDirectory>,
}

impl Peer {
    /// Constructs a `Peer`, stamping `last_seen` with the current time
    /// and stripping the trailing `.local` suffix that mDNS appends to
    /// hostnames.
    pub fn new(
        id: Uuid,
        addr: IpAddr,
        hostname: String,
        instance_id: Uuid,
        sync_dirs: Vec<SyncDirectory>,
    ) -> Self {
        let hostname = hostname
            .strip_suffix(".local")
            .unwrap_or(&hostname)
            .to_string();
        let sync_dirs = sync_dirs
            .into_iter()
            .map(|dir| (dir.name.clone(), dir))
            .collect();

        Self {
            id,
            instance_id,
            addr,
            hostname,
            sync_dirs,
            last_seen: SystemTime::now(),
        }
    }
}
