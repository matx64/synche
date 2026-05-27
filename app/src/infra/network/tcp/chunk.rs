pub(super) const TRANSFER_CHUNK_SIZE: usize = 1024 * 1024;
pub(super) const MAX_TRANSFER_SIZE: u64 = 16 * 1024 * 1024 * 1024;

/// Upper bound on a handshake JSON payload length advertised by a peer.
/// Handshakes carry the full entry map, so the cap is generous, but a
/// `u32` length field with no ceiling lets any LAN host force a ~4 GiB
/// allocation per connection.
pub(super) const MAX_HANDSHAKE_JSON_SIZE: usize = 8 * 1024 * 1024;

/// Upper bound on a single `EntryInfo` JSON payload length advertised
/// by a peer. One entry should be well under 64 KiB.
pub(super) const MAX_ENTRY_JSON_SIZE: usize = 64 * 1024;
