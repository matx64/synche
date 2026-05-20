use std::collections::HashMap;
use uuid::Uuid;

/// Per-device monotonic counters that drive Synche's conflict
/// resolution.
///
/// Keys are device `local_id`s; values are bumped on every local
/// change. Two versions are comparable per `EntryInfo::compare`.
pub type VersionVector = HashMap<Uuid, u64>;

/// The outcome of comparing two `EntryInfo` version vectors.
///
/// `Conflict` is not an error condition — it means the two sides have
/// concurrent edits and the caller must materialize a conflict file
/// rather than choose a winner.
pub enum VersionCmp {
    /// Same kind and same hash — nothing to do.
    Equal,
    /// Local version dominates the remote one; keep the local entry.
    KeepSelf,
    /// Remote version dominates the local one; adopt the remote entry.
    KeepOther,
    /// Concurrent edits on both sides; preserve both by writing a
    /// conflict file.
    Conflict,
}
