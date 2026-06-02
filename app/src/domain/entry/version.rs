use std::collections::HashMap;
use uuid::Uuid;

/// Per-device monotonic counters that drive Synche's conflict
/// resolution.
///
/// Keys are device `local_id`s; values are bumped on every local
/// change. Two versions are comparable per `EntryInfo::compare`.
pub type VersionVector = HashMap<Uuid, u64>;

/// Upper bound on a peer-supplied version counter that we'll merge.
///
/// Honest counters bump by one per local edit, so they grow slowly.
/// A peer advertising anything past half of `u64::MAX` is either
/// broken or hostile, so the merge boundary rejects it rather than
/// poisoning our state. Half-range leaves plenty of headroom for the
/// `checked_add` paths in `EntryManager`.
pub const MAX_TRUSTED_COUNTER: u64 = u64::MAX / 2;

/// The outcome of comparing two `EntryInfo` version vectors.
///
/// `Conflict` is not an error condition — it means the two sides have
/// concurrent edits and the caller must materialize a conflict file
/// rather than choose a winner.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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
