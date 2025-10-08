use uuid::Uuid;

pub struct WsEvent {
    kind: WsEventKind,
}

pub enum WsEventKind {
    PeerConnected(Uuid),
    PeerDisconnected(Uuid),
    SyncDirectoryUpdate(SyncDirectoryUpdateData),
}

pub struct SyncDirectoryUpdateData {
    kind: SyncDirectoryUpdateKind,
    name: String,
}

pub enum SyncDirectoryUpdateKind {
    Ok,
    Syncing,
}
