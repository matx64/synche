use std::collections::HashMap;
use uuid::Uuid;

pub type VersionVector = HashMap<Uuid, u64>;

pub enum VersionCmp {
    Equal,
    KeepSelf,
    KeepOther,
    Conflict,
}
