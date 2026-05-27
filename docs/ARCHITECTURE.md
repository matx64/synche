# Architecture Guide

This document describes the sync engine internals: the hexagonal layer structure, conflict resolution via version vectors, the TCP wire format, the `home_path` restart contract, and mDNS peer discovery.

---

## Hexagonal Architecture Layout

Synche follows a **ports-and-adapters** (hexagonal) layout.  The three layers have a strict one-way dependency: `domain` ← `application` ← `infra`.  No layer may reach across or skip a boundary.

```
app/src/
├── domain/          ← pure types, no I/O, no async
├── application/     ← services + port traits
└── infra/           ← concrete adapters
```

### `domain/`

Pure Rust types with no I/O and no async.  The full domain surface is re-exported from [`app/src/domain/mod.rs`](../app/src/domain/mod.rs):

- `Config`, `SyncDirectory`, `AppPorts`
- `Peer`
- `EntryInfo`, `EntryKind`, `VersionVector`, `VersionCmp`
- `CanonicalPath`, `RelativePath`
- `ServerEvent`
- `TransportData`, `TransportEvent`, `TransportMetadata`, `HandshakeData`
- Channel helpers: `BroadcastChannel`, `MutexChannel`

### `application/`

Services and the **port traits** they depend on.  Each subsystem defines its trait in an `interface.rs`:

| Trait | Location |
|-------|----------|
| `FileWatcherInterface` | [`application/watcher/interface.rs`](../app/src/application/watcher/interface.rs) |
| `TransportInterface` | [`application/network/transport/interface.rs`](../app/src/application/network/transport/interface.rs) |
| `PresenceInterface` | [`application/network/presence/interface.rs`](../app/src/application/network/presence/interface.rs) |
| `PersistenceInterface` | [`application/persistence/interface.rs`](../app/src/application/persistence/interface.rs) |

The services that consume those ports are `FileWatcher`, `TransportService`, `PresenceService`, `EntryManager`, and `PeerManager`.  The top-level orchestrator is `Synchronizer` ([`application/sync.rs`](../app/src/application/sync.rs)).

### `infra/`

Concrete adapters wired up in `Synchronizer::new_default()`:

| Adapter | Port satisfied | Source |
|---------|---------------|--------|
| `NotifyFileWatcher` | `FileWatcherInterface` | [`infra/watcher/notify.rs`](../app/src/infra/watcher/notify.rs) |
| `MdnsAdapter` | `PresenceInterface` | [`infra/network/mdns.rs`](../app/src/infra/network/mdns.rs) |
| `TcpAdapter` | `TransportInterface` | [`infra/network/tcp/`](../app/src/infra/network/tcp/) |
| `SqliteDb` | `PersistenceInterface` | [`infra/persistence/sqlite.rs`](../app/src/infra/persistence/sqlite.rs) |
| HTTP server | (GUI + API) | [`infra/http/`](../app/src/infra/http/) |

### Runtime wiring

`AppState` ([`application/state/app_state.rs`](../app/src/application/state/app_state.rs)) is an `Arc<AppState>` shared across all tasks.  It carries device IDs, the peer map, the sync-dir map, port numbers, `home_path`, the local IP, and the SSE broadcast channel.

`Synchronizer::run` joins four concurrent tasks via `tokio::select!`: transport service, presence service, file watcher, and HTTP server.

---

## Version Vectors and Conflict Resolution

> **Source:** [`app/src/domain/entry/version.rs`](../app/src/domain/entry/version.rs) · [`app/src/application/entry_manager.rs`](../app/src/application/entry_manager.rs)

### VersionVector

```rust
pub type VersionVector = HashMap<Uuid, u64>;
```

A `VersionVector` maps each device's persistent `local_id` (UUID) to a monotonically increasing counter.  The counter is bumped every time a device makes a local change to an entry.

### VersionCmp

Comparing two `EntryInfo` values yields one of four outcomes:

| Variant | Meaning |
|---------|---------|
| `Equal` | Same kind and same hash — nothing to do |
| `KeepSelf` | Local version strictly dominates remote; discard the remote entry |
| `KeepOther` | Remote version strictly dominates local; adopt the remote entry |
| `Conflict` | Concurrent edits on both sides — neither dominates; preserve both |

The comparison iterates over the union of all device keys in both vectors.  If local counters are strictly greater on every key → `KeepSelf`; strictly lower → `KeepOther`; mixed → `Conflict`.

### Conflict materialization

`Conflict` is not an error — it means both sides have valid but diverging histories.  The `EntryManager` resolves conflicts deterministically:

1. **Removed vs. live:** The live (non-removed) side always wins.  If both are removed the result is `Equal`.
2. **Both live — tiebreak:** The device with the **lower UUID** keeps its version.  The other device copies its local file to a conflict file and then accepts the winner's bytes.

**Conflict file naming:**

```
<stem>_CONFLICT_<unix_epoch_seconds>_<device_uuid>.<ext>
```

Example: `report_CONFLICT_1716864000_a1b2c3d4-e5f6-47g8-h9i0-j1k2l3m4n5o6.md`

This naming ensures no data is lost and the conflict file is unambiguously associated with the device that created it.

### Deletion sentinel

Deleted entries are not removed from the metadata store.  Instead, their `hash` field is set to the 32-character all-zeros string `"00000000000000000000000000000000"` (`REMOVED_HASH`).  This sentinel allows the version vector to keep propagating the deletion to peers that missed it.

---

## TCP Wire Format

> **Source:** [`app/src/infra/network/tcp/`](../app/src/infra/network/tcp/)

Every peer-to-peer message uses the same framing.  The sender opens a fresh TCP connection for each message.

### Frame layout

```
Bytes  0–15   Source device UUID (16 raw bytes, big-endian UUID representation)
Byte   16     Kind tag (1 byte, see table below)
Bytes  17–20  Payload length L (u32 big-endian)
Bytes  21–    JSON payload (L bytes)

--- Transfer frames only ---
Bytes  21+L – 21+L+7   File size S (u64 big-endian)
Bytes  21+L+8 –        Raw file data (exactly S bytes, streamed in 1 MiB chunks)
```

### Kind tags

| Value | Variant | Payload type |
|-------|---------|--------------|
| `1` | `HandshakeSyn` | `HandshakeData` (JSON) |
| `2` | `HandshakeAck` | `HandshakeData` (JSON) |
| `3` | `Metadata` | `EntryInfo` (JSON) |
| `4` | `Request` | `EntryInfo` (JSON) |
| `5` | `Transfer` | `EntryInfo` (JSON) + raw file bytes |

The discriminants are part of the wire format — changing them would break compatibility with older peers.

### Handshake flow

When a peer is first discovered via mDNS, the local device opens a TCP connection and sends a `HandshakeSyn`.  The peer replies with a `HandshakeAck` on a new outbound connection.  Both messages carry a `HandshakeData` payload:

```json
{
  "hostname": "laptop",
  "instance_id": "<per-process UUID>",
  "sync_dirs": [{ "name": "Photos" }],
  "entries": {
    "Photos/vacation.jpg": { "name": "Photos/vacation.jpg", "kind": "File", "hash": "abc123...", "version": { "<uuid>": 3 } }
  }
}
```

After the handshake, each side compares the received entry map against its own and requests any entries where the peer's version dominates.

### Metadata and Request messages

Both use the short frame (no file bytes):

- **Metadata** — unidirectional announcement of an `EntryInfo` change, broadcast to all peers after any local file event.
- **Request** — asks the target peer to send a `Transfer` for the named entry.

### Chunked file transfer

`Transfer` frames stream file bytes in **1 MiB chunks** (constant `TRANSFER_CHUNK_SIZE = 1024 * 1024`) with a streaming SHA-256 computed over the bytes actually sent.  The maximum supported transfer size is **16 GiB** (`MAX_TRANSFER_SIZE = 16 * 1024 * 1024 * 1024`).

If the source file shrinks during streaming the remaining bytes are zero-padded so the wire size matches the advertised `S`.  The hash will diverge and the receiver rejects the transfer by hash mismatch.

### Error handling

Errors that occur **after** a connection is accepted (corrupt payload, truncated stream, malformed JSON) are logged and skipped — they do not stop the synchronizer.  Listener bind and accept failures remain fatal.

---

## `home_path` Change → Restart Sentinel Contract

> **Source:** [`app/src/application/sync.rs`](../app/src/application/sync.rs)

Changing `home_path` via [`POST /api/set-home-path`](API.md#post-apiset-home-path) writes the new value to `config.toml` and then signals the synchronizer to rebuild by propagating a sentinel `io::Error`:

```
HOME_PATH_CHANGED:<old_path>:<new_path>
```

### Live activity events

The transport receiver broadcasts per-entry SSE events as files move across the network:

- `EntrySyncStarted` is emitted from `TransportReceiver` at both points where this device enqueues a `Request` (handshake catch-up and the `KeepOther` branch of `handle_metadata`).
- `EntrySyncCompleted` is emitted after `handle_transfer` calls `insert_entry`.
- `EntrySyncFailed` is emitted from `TcpReceiver::read_transfer` once the entry header has been parsed; the original `TransportError` is still propagated up so the receive loop logs and skips it as a bad peer message.

The GUI renders these into a per-directory activity strip with a rolling history of recent completed/failed entries.  See [API.md](API.md#server-sent-events) for the wire schemas.

### Restart flow

1. The API handler calls `AppState::set_home_path_in_config()`, which writes the config and returns the sentinel error.
2. The sentinel bubbles up through `Synchronizer::_run()` and is caught by `Synchronizer::run()`.
3. Before returning the error, `run()` broadcasts a `ServerRestart` SSE event (allowing the GUI to reconnect) and then shuts down the current synchronizer instance.
4. `run_default_with_restart()` receives the sentinel, logs the path change, and re-enters the loop to build a new `Synchronizer` with the updated configuration.
5. The OS process is **not** restarted — only the in-process synchronizer is rebuilt.

### Parsing

The sentinel is parsed with `split_once(':')` so that colons in the new path (e.g. Windows drive letters like `C:\Users\...`) are preserved correctly.

### Contract

Any code that touches the shutdown or restart path **must not** swallow, modify, or re-wrap this sentinel error.  Only `run_default_with_restart` may consume it.

---

## mDNS Peer Discovery

> **Source:** [`app/src/infra/network/mdns.rs`](../app/src/infra/network/mdns.rs)

Synche uses multicast DNS (mDNS) for zero-configuration peer discovery on the local network.

### Service type

```
_synche._udp.local.
```

### Service advertisement

Each device registers itself as:

```
<local_id>._synche._udp.local.
```

where `<local_id>` is the persistent device UUID.  The registration includes:

- **Host:** `<hostname>.local.`
- **Address:** local IPv4 address (IPv6 is disabled)
- **Port:** presence port (default **42881**)
- **TXT property `instance_id`:** a per-process UUID generated fresh on each startup

### Peer discovery loop

The `MdnsAdapter` browses for `_synche._udp.local.` records and converts `ServiceEvent`s into `PresenceEvent`s:

| `ServiceEvent` | `PresenceEvent` |
|----------------|-----------------|
| `ServiceResolved` | `Ping { id, addr, instance_id }` |
| `ServiceRemoved` | `Leave { id }` |

Loopback addresses and the device's own `local_id` are filtered out.

### Restart detection

Because `instance_id` changes on every process start, a peer that re-advertises with a different `instance_id` is treated as a fresh connection.  This ensures that a peer's handshake is repeated after it restarts even if its `local_id` (and therefore its mDNS service name) stays the same.
