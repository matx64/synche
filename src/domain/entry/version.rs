use std::collections::HashMap;
use uuid::Uuid;

pub type VersionVector = HashMap<Uuid, u64>;

pub enum VersionVectorCmp {
    Equal,
    KeepSelf,
    KeepPeer,
    Conflict,
}
