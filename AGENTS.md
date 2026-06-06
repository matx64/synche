# AGENTS.md

This file provides guidance to agents when working with code in this repository. IMPORTANT: Update this file and add tests on every relevant change.

## Commands

The single Cargo workspace member is `app` (binary name `synche`).

- `cargo run -p synche` — run a debug build.
- `cargo build --release` — release build; binary at `target/release/synche`.
- `cargo test -p synche` — run all tests.
- `cargo test -p synche <name>` — run a single test by name substring (e.g. `cargo test -p synche test_validate_home_path_relative_path`).
- `cargo fmt` / `cargo clippy` — required to be clean before PRs (per `docs/CONTRIBUTING.md`).
- `just dev` — runs `watchexec` to restart `cargo run -p synche` whenever `app/` or `gui/index.html` changes. Requires `just` and `watchexec` installed.
- `just setup-hooks` — installs the git pre-commit hook (run once after cloning).

Running the binary serves the web GUI at `http://localhost:42880`. Default ports: HTTP `42880`, presence (mDNS) `42881`, transport (TCP) `42882` — the `DEFAULT_*_PORT` constants in `app/src/application/state/app_state.rs`. They are no longer fixed: ports resolve with precedence **CLI flag > `[ports]` block in `config.toml` > default** (see the "Configurable ports & CLI flags" section below).

## Pre-commit checklist (mandatory)

Run both commands after **every** change — no exceptions:

```sh
cargo clippy -p synche -- -D warnings
cargo test -p synche
```

Both must exit with **zero warnings and zero failures**.

- `-D warnings` promotes every Clippy warning to a hard error so nothing slips through.
- If either command fails, fix the root cause before marking the task done.
- Never silence a warning with `#[allow(...)]` without explicit approval from the user.

**Enforcement split**: the local `pre-commit` hook (`.githooks/pre-commit`) runs `cargo fmt --check` + `cargo clippy` only; `cargo test` is enforced by CI (`.github/workflows/ci.yml`) on every push/merge to `main`. The agent checklist above (clippy **and** test before done) is unchanged — still run both locally regardless of what the hooks gate.

## Architecture

Single Cargo workspace (root `Cargo.toml`) with one member crate at `app/`. Rust source lives under `app/src/` and follows a **hexagonal / ports-and-adapters** layout. Read this section before navigating individual files — the layer boundaries matter.

### Layers

- **`domain/`** — pure types, no I/O, no async. The full domain surface is re-exported from `app/src/domain/mod.rs`: `Config`, `Peer`, `EntryInfo`/`EntryKind`, `VersionVector`/`VersionCmp`, `CanonicalPath`/`RelativePath`, `SyncDirectory`, `AppPorts`/`PortOverrides`, `ServerEvent`, the `Transport*` family, and channel helpers (`BroadcastChannel`, `MutexChannel`).
- **`application/`** — services and the **traits (ports)** they depend on. Each subsystem defines its trait in an `interface.rs`:
  - `application/watcher/interface.rs` → `FileWatcherInterface`
  - `application/network/transport/interface.rs` → `TransportInterface`
  - `application/network/presence/interface.rs` → `PresenceInterface`
  - `application/persistence/interface.rs` → `PersistenceInterface`

  Services consuming those ports: `FileWatcher`, `TransportService`, `PresenceService`, `EntryManager`, `PeerManager`. The top-level orchestrator is `application::Synchronizer` (`app/src/application/sync.rs`).
- **`infra/`** — concrete adapters implementing the application ports. The defaults wired in `Synchronizer::new_default()`:
  - `NotifyFileWatcher` (`infra/watcher/notify.rs`) — `notify` crate
  - `MdnsAdapter` (`infra/network/mdns.rs`) — `mdns-sd`
  - `TcpAdapter` (`infra/network/tcp/`) — TCP transport
  - `SqliteDb` (`infra/persistence/sqlite.rs`) — `sqlx` + SQLite
  - HTTP server / GUI in `infra/http/` (axum + minijinja + tower-http static serving)

**Dependency rule**: `domain` knows nothing about `application` or `infra`. `application` knows `domain` and defines traits. `infra` depends on `application` traits and `domain` types. Don't reach across — add or extend a port instead.

### Runtime wiring

`AppState` (`app/src/application/state/app_state.rs`) is the shared `Arc<AppState>` carrying device IDs, peer map, sync-dir map, ports, `home_path`/`local_ip`, and the SSE broadcast channel. `Synchronizer::run` joins four concurrent tasks via `tokio::select!`: the transport service, presence service, file watcher, and HTTP server.

`run_default_with_restart` wraps `run` in a loop that catches a sentinel `io::Error` whose message starts with `HOME_PATH_CHANGED:<old>:<new>` and rebuilds the entire `Synchronizer`. This is how a `home_path` change made through the GUI is applied at runtime — preserve that contract when touching shutdown/restart paths.

TCP transport receive errors after a connection is accepted are treated as bad peer messages and skipped so a corrupt transfer, truncated stream, or malformed payload does not stop the synchronizer. Listener bind/accept failures remain fatal.

### Configurable ports & CLI flags

The binary parses CLI args with `clap` in `main.rs` via the `Cli` struct in `app/src/cli.rs` (`--config-dir`, `--http-port`, `--presence-port`, `--transport-port`; `--version`/`--help` are auto-generated, and `--version` reads `CARGO_PKG_VERSION` so the single-source rule holds). The three port flags are collected into a `domain::PortOverrides` (a struct of three `Option<u16>`), the same type the optional `[ports]` block in `config.toml` deserializes into.

Port resolution lives in `app_state::resolve_ports(cli, cfg)` and runs inside `AppState::new`, which already loads the config: each port is `cli.x.or(cfg.x).unwrap_or(DEFAULT_X)`, so precedence is **CLI > config > default** and each port resolves independently. `PortOverrides` is threaded from `main` through `Synchronizer::run_default_with_restart` / `new_default_with_dirs` so CLI ports survive the `home_path`-change restart loop. There is no `default_ports()` helper anymore — resolution is the only path. Tests build ports via `PortOverrides::ephemeral()` (all `Some(0)`).

Transport routing must use the **remote peer's** advertised endpoint, not `SocketAddr::new(peer_ip, state.ports().transport)`. mDNS keeps the service record's port as the presence port and advertises the TCP transport port in TXT key `transport_port`; `PresenceEvent::Ping`, `TransportChannelData`, and `TransportInterface::send` carry `SocketAddr`s. Handshake JSON also includes optional `transport_port` so a peer receiving a SYN can send the ACK back to the sender's listening port. Missing old-peer values fall back to `DEFAULT_TRANSPORT_PORT`.

`[ports]` is `#[serde(default, skip_serializing_if = "PortOverrides::is_empty")]` on `Config`, so a missing block deserializes to empty and the auto-generated default `config.toml` omits it entirely. **`AppState` keeps only the resolved `AppPorts`, not the original config block**, so the three GUI-driven config rewrites (`add_dir_to_config`, `remove_dir_from_config`, `set_home_path_in_config`) must re-read the on-disk `[ports]` via `AppState::current_config_ports()` when rebuilding `Config` — otherwise a rewrite would silently wipe a user's `[ports]` block.

`--config-dir <path>` relocates **all** state, not just config: it builds `SyncheDirs::rooted_at(path)` (`<path>/{data,config,logs}`), the same constructor tests use. This full isolation (separate `data.db` **and** `device_id`) is what lets two instances run side-by-side on one host — sharing `device_id` would otherwise trip the self-handshake guard. When `--config-dir` is absent, `SyncheDirs::from_os()` is used as before.

### Conflict resolution

`VersionVector = HashMap<Uuid, u64>` keyed by device `local_id` (`app/src/domain/entry/version.rs`). Comparing two versions yields `VersionCmp::{Equal, KeepSelf, KeepOther, Conflict}` — concurrent edits produce `Conflict` (which the system materializes as a conflict file) rather than overwriting. Anything that mutates `EntryInfo` or decides which side wins must go through this comparison.

`EntryManager::handle_conflict` writes the losing local file aside as `<stem>_CONFLICT_<unix_ms>_<device_uuid>_<random>.<ext>` and **must** use `fs::OpenOptions::create_new(true)` (never `fs::copy`, which truncates), retrying with a fresh random suffix on `AlreadyExists`. Millisecond resolution alone is not collision-proof — the per-attempt random component is what guarantees two conflicts for the same file in the same second from the same peer don't overwrite each other; the conflict-recovery path itself must not lose data. Conflict copies still re-enter sync and propagate to peers like any other new file.

### Conflict listing & resolution (GUI)

The `<stem>_CONFLICT_<ms>_<device>_<rand>.<ext>` format is **single-sourced** by `RelativePath::conflict_file_name` (`app/src/domain/fs/path.rs`); `handle_conflict` builds names through it, and `RelativePath::is_conflict_file` / `conflict_origin` parse them back (validating the trailing `<ms>_<uuid>_<8-hex>` triple precisely so `.gitignore` and user files that merely contain `_CONFLICT_` are not misclassified). Changing the layout means changing all three together.

`EntryManager::list_conflicts` surfaces live (non-tombstone) conflict-copy file entries inside configured sync dirs, grouped per dir by the HTTP layer for `GET /api/conflicts`. `EntryManager::resolve_conflict(conflict, ResolveAction)` performs the resolution and returns the entries to broadcast: **KeepMine** deletes the copy and tombstones it; **KeepTheirs** overwrites the original (temp-sibling → rename, never copy over the live file), bumps it via `entry_created`, then deletes + tombstones the copy. It mirrors the receiver's discipline — shared path-mutation gate, per-entry inflight lock(s), and `mark_remote_write`/`clear_remote_write` (cleared unconditionally, never from `Drop`) around the disk writes. The HTTP layer threads the transport `sender_tx` (added to `infra::http::run` → `build_router` → `api::routes`, stored on `Synchronizer`) so `POST /api/resolve-conflict` can broadcast the returned `Metadata` to peers — the GUI is otherwise outside the watcher/transport wiring.

`ConflictDetected` / `ConflictResolved` SSE fire from five guarded sites (each a one-line `AppState::broadcast_conflict_detected`/`broadcast_conflict_resolved` call): watcher create (creator side), watcher remove (manual delete), `TransportReceiver::handle_transfer` commit (peer receives a copy), `apply_peer_tombstone` (peer of a resolver), and `resolve_conflict` itself. Both sides therefore update live for both transitions. The GUI refetches `GET /api/conflicts` on either event rather than diffing — grouping stays single-sourced in `api.rs`.

### Permanent exclusions

Permanent path exclusions must be enforced at every boundary where entries can enter or leave sync: filesystem scans, watcher events, handshakes, metadata handling, request handling, transfer handling, and disk writes. Use `utils::fs::is_git_path` as the shared predicate for `.git/` path exclusion. It matches an exact `.git` path component only, so `.gitignore`, `.gitattributes`, `.github/`, and `foo.git/` remain syncable.

Remote transport paths must be validated before any metadata handling or disk write. Use `RelativePath::is_safe_sync_path` to reject absolute paths, parent-directory traversal, empty paths, backslash-separated paths, and paths with embedded NUL bytes from peers.

### Scoping inbound entries to configured sync_dirs

Every inbound `Metadata`/`Request`/`Transfer` handler in `TransportReceiver` (`app/src/application/network/transport/receiver.rs`) drops entries whose top component is not a configured sync directory. The guard is `TransportReceiver::is_in_configured_sync_dir`, which delegates to `AppState::contains_sync_dir`. The same check already runs in `EntryManager::get_entries_to_request` and `build_db`. `TcpReceiver` must also enforce the configured-sync-dir guard before staging or finalizing inbound `Transfer` bytes, because application-layer filtering happens after the TCP adapter has decoded the frame. If you add a new inbound entry path, add the guard alongside the existing `is_git_path` filter — they belong together.

"Path under sync dir" checks must be component-aware: use `RelativePath::starts_with_dir`, never `str::starts_with` on a `RelativePath`. The dereference-to-`str` makes it look right, but `foo` would then match `foobar/...`.

### Inbound TCP message size caps

`app/src/infra/network/tcp/chunk.rs` defines three hard caps that are enforced **before** allocating: `MAX_TRANSFER_SIZE` (raw file bytes), `MAX_HANDSHAKE_JSON_SIZE` (handshake JSON), `MAX_ENTRY_JSON_SIZE` (single `EntryInfo` JSON). Anything that decodes a peer-supplied `u32` length must check it against the right cap before `vec![0u8; len]`. Don't add a new variable-length frame without picking (or adding) a cap.

The TCP frame decoder (`receiver.rs`/`adapter.rs`) and `RelativePath::is_safe_sync_path` (`path.rs`) are the wire-facing security edges, so they carry table-driven malformed-input tests: oversized/`u32::MAX` length prefixes, streams truncated mid-frame, invalid UTF-8 in string fields, embedded NUL bytes, and unsafe paths (absolute, `..` traversal, backslash/mixed separators). Each case must either be rejected by `is_safe_sync_path`/`validate_*` or drained/dropped by the receiver so the adapter keeps serving other peers. When you add a new frame field or relax a path rule, extend those tables.

### Sanitizing peer-supplied version vectors

Any path that persists a peer-supplied `EntryInfo` must strip foreign axes and reject counters above `MAX_TRUSTED_COUNTER` first. The hardened paths are `EntryManager::merge_versions_and_insert` (on the `Equal | KeepSelf` branch of `compare_and_resolve_conflict`) and `EntryManager::insert_peer_entry` / `insert_peer_tombstone` (used by `TransportReceiver::handle_transfer` via `commit_staged_transfer`, `create_received_dir` for accepted directory-create entries, and accepted remote tombstones). When replacing an existing row with a peer entry, preserve the trusted existing version vector and merge only the sender's own inbound axis; never reset the local axis to zero after accepting a transfer. `TcpReceiver` must reject or drain-and-drop poisoned `Transfer` frames before staging bytes, since the file payload is materialized before metadata is persisted. `TransportReceiver::handle_transfer` must also reject poisoned counters before `take_pending_request`, because the TCP layer returns drained-but-unstaged Transfer events and a rejected frame must not burn the legitimate pending request. `TcpReceiver` also preclaims a matching pending request via `AppState::claim_pending_request_for_staging` before creating staging, and releases that claim if staging or hash validation fails before the app sees the event. Plain `insert_entry` is for trusted local writes only — never call it directly on a peer-supplied entry.

Comparison decisions must use the same sanitized peer view before calling `EntryInfo::compare`. Do not let foreign axes influence `handle_metadata`, handshake request selection, conflict resolution, delete decisions, or transfer requests; sanitize to the sender's own axis first, then persist through the hardened paths above.

### Conflict-resolved-as-KeepSelf must NOT merge the peer's axis

When `compare_and_resolve_conflict` sees `VersionCmp::Conflict` and `handle_conflict` returns `KeepSelf` (the local-id tiebreak made us the winner), do **not** call `merge_versions_and_insert`. Absorbing the peer's counter under an axis whose content we never integrated would make our vector dominate the peer's on the next exchange — and the peer would then silently overwrite its own edit with no conflict file on either side. The current code threads the raw compare result alongside the post-`handle_conflict` outcome so the merge runs only on `Equal` or a non-conflict `KeepSelf`. Preserve that invariant.

### Durable tombstones

`EntryManager::delete_and_update_entry` must persist local tombstones via `db.insert_or_replace_entry` (bumped local counter + `EntryInfo::mark_removed`, which sets the explicit `deleted` flag and clears `hash`), not call `db.delete_entry`. Runtime and persisted tombstone state is the `deleted: bool` field on `EntryInfo`; the live hash namespace must never use an in-band tombstone sentinel. The legacy `REMOVED_HASH` 32-zero string survives only as a `LEGACY_REMOVED_HASH` compatibility marker for SQLite migration, inbound `EntryInfo` deserialization from older peers, and outbound JSON serialization of tombstones for mixed-version peers. Inbound/migration paths must normalize it to `deleted = true` with `hash = None`, and serialization must not mutate the runtime `hash`. The deleted-column migration must be idempotent after a partial crash, so sentinel promotion and deleted-row hash clearing still run when the column already exists. Accepted peer tombstones must go through `insert_peer_tombstone`, including when no local row exists yet, so the peer's delete remains durable without fabricating a local delete counter. Accepted peer tombstones must also acquire the same per-entry inflight lock used by `commit_staged_transfer`, re-run `handle_metadata` while holding that lock, and only persist/remove/broadcast if the fresh comparison still returns `KeepOther`; otherwise an older in-flight Transfer can overwrite a newer tombstone after the tombstone removes the file. When an accepted peer tombstone is for a **directory**, `apply_peer_tombstone`'s `remove_path_from_disk` does `fs::remove_dir_all` and wipes the whole subtree on disk, but only the single named row was tombstoned — so it must also durably tombstone every descendant row via `EntryManager::tombstone_dir_descendants` (which authors a local-axis tombstone per strict descendant through `delete_and_update_entry`, mirroring the local watcher's `remove_dir`), inside the same inflight-lock + remote-write-mark window, and broadcast each. Directory tombstones must hold AppState's exclusive path-mutation gate while applying the subtree delete; transfers and file tombstones hold the shared gate before consuming pending Transfer claims and before taking their exact per-entry lock, and `commit_staged_transfer` must reject writes under a tombstoned ancestor directory. Without this, the in-memory window before the peer's per-child tombstones arrive lets a handshake re-advertise a still-live descendant to a peer that still holds the live copy, or lets a concurrently staged child Transfer recreate the removed subtree. `build_db`'s "file missing on disk" branch must keep rows for which `entry.is_removed()` is true so a tombstone survives restart and continues propagating to peers via the handshake entry list. If a filesystem entry exists for a tombstoned row during startup, or a watcher create/modify event observes a tombstoned path recreated locally, the live entry must clear `deleted`, update `kind`/`hash`, bump the local axis, and broadcast live metadata so the resurrection dominates the prior tombstone. Handshake reconciliation must apply `entry.is_removed()` before checking `entry.is_file()`; tombstones keep their original `kind` but carry the `deleted` flag, and must never enqueue a file `Request`. Removing a sync directory from config is not a filesystem delete: `EntryManager::remove_sync_dir` must purge local metadata with `db.delete_entry` and must not create tombstones or advertise removed entries to peers. Without durable tombstones for real deletes, a crash between the row-delete and metadata broadcast — or any late-joining peer — would silently re-sync the deleted file back from a peer that still has the live copy.

Tombstones are GC'd on a fixed retention window so the entry map — and every handshake payload — does not grow without bound. The retention timestamp lives **only** in the persistence layer as a nullable `tombstoned_at` (Unix millis) column on the SQLite `entries` table — it is **not** added to the domain `EntryInfo` nor to the wire/JSON format, so no construction site, serialization, or peer-compatibility path changes. `SqliteDb::insert_or_replace_entry` is an `INSERT ... ON CONFLICT DO UPDATE` UPSERT (not `INSERT OR REPLACE`, which would delete the prior row and reset the stamp): a fresh tombstone is stamped `now`, a re-persisted tombstone preserves its original stamp via `COALESCE(entries.tombstoned_at, excluded.tombstoned_at)`, and a row going live clears it to `NULL`. `migrate_tombstoned_at_column` adds the column idempotently and backfills existing tombstones with `now` so older deletes become GC-eligible from upgrade time rather than being dropped immediately. `EntryManager::gc_tombstones(retention)` computes `cutoff = now - retention` and delegates to `db.gc_tombstones(cutoff)` (`DELETE ... WHERE deleted = 1 AND tombstoned_at IS NOT NULL AND tombstoned_at < cutoff`). `Synchronizer::run_tombstone_gc` runs the sweep as a fifth `tokio::select!` arm (`TOMBSTONE_RETENTION` = 30 days, `TOMBSTONE_GC_INTERVAL` = 6 h, first tick fires at startup); GC errors are logged and swallowed so a transient DB failure cannot tear down the synchronizer. The tradeoff is the fixed window: a peer offline longer than the retention period could resurrect a deleted file, the same hazard durable tombstones guard against. Ack-based GC (drop only once every known peer has seen the tombstone) remains a deliberate follow-up — there is no per-entry peer-ack tracking today.

### Pre-rename validation of inbound Transfers

`TcpReceiver` writes verified Transfer bytes to a per-transfer staging directory in the OS temp dir and returns a `StagedTransfer` RAII guard on the `TransportEvent`; it **does not** rename into `home_path`. `Transfer` frames are valid only for live file entries with a content hash; tombstones, directories, and hashless file entries must be drained/dropped before staging and rejected again by the application/commit gates without consuming the pending request. Tombstones propagate only through `Metadata` and handshakes. The application layer commits via `EntryManager::commit_staged_transfer`, which runs four checks before moving bytes into place and then persisting metadata:

1. `is_git_path` and `AppState::contains_sync_dir` (defense in depth — already enforced at the TCP layer pre-stage).
2. `AppState::take_pending_request(peer_id, name)` — every legitimate Transfer is preceded by a `Request` this device registered via `AppState::register_pending_request` in `handle_metadata` / `handle_handshake`. The TCP adapter consumes that pending request into a one-shot staging claim before writing bytes; `handle_transfer` then consumes the claim before commit. In-memory transports may still consume a live pending request here. Unsolicited TCP transfers are drained without staging, and app-layer unsolicited transfers are dropped without touching `home_path`.
3. `local.compare(sanitized_peer)` must yield `Equal`, `KeepOther`, or `Conflict→KeepOther`. A `KeepSelf` outcome drops the staged bytes; the local edit is preserved.
4. `AppState::acquire_inflight_lock(name)` serializes concurrent commits of the same path, and accepted peer tombstones for that path use the same lock before revalidating and applying the delete. Always pair it with `release_inflight_lock` when done.

On any failure path before the move, the `StagedTransfer` is dropped and its `Drop` impl synchronously cleans up the staging directory. Do not bypass `commit_staged_transfer` to write Transfer bytes directly into `home_path`. If `fs::rename(staging, target)` reports `CrossesDevices`, copy to a temporary sibling inside the target directory and then rename that temp file to the final target; never copy directly over the user file. Metadata must be written only after the final target file has been replaced, avoiding a DB-new/disk-old crash state; if metadata persistence fails after the move, startup/watch reconciliation can recover from the disk-new/DB-old state.

### Hashing must not race a concurrent local writer

`compute_hash` (`app/src/utils/fs.rs`) snapshots a cheap `(mtime, size)` signature via `file_signature` **before and after** streaming the file, and recomputes when the signature changed (bounded by `MAX_ATTEMPTS`). Without this, hashing a file a local process is still writing would publish a hash that doesn't match the on-disk bytes: every peer that `Request`s it rejects the transfer (`computed_hash != *entry.hash` in `infra/network/tcp/receiver.rs`) and the stale hash lingers until the next watcher event recomputes it. After `MAX_ATTEMPTS` unstable reads it returns a best-effort hash — the next watcher event recomputes once the writer settles. The retry is internal to `compute_hash`, so all call sites (watcher create/modify, `build_dir` scan) are covered without change. `modified()` is best-effort (`.ok()`); platforms without an mtime fall back to size-only detection.

### Watcher must not race the synchronizer's own disk writes

The file watcher does **not** hold the per-entry inflight lock, so a remote-driven disk write can fire a `notify` event the watcher would misread as a local edit. Before mutating a path on behalf of a peer, the synchronizer marks it via `AppState::mark_remote_write` and clears it via `AppState::clear_remote_write`; the watcher skips any path for which `AppState::is_remote_write_in_progress` is true before inserting adapter events into the debounce buffer, with `handle_entry_create_or_modify` and `handle_entry_remove` retaining the same guard as defense in depth. The predicate is component-aware and matches the marked path plus ancestors/descendants, so nested Transfer parent-directory creates and peer directory-tombstone child removes are suppressed too. This closes the window the `file.hash != disk_hash` check in `handle_modify_file` leaves open — without it, a watcher event processed between the byte move and the metadata persist would bump the local counter and broadcast a spurious local-edit `Metadata`, polluting every peer's version vector. Two paths mark/clear: `EntryManager::commit_staged_transfer` around the move→persist window, and `TransportReceiver::apply_peer_tombstone` around the file removal (its tombstone metadata is already persisted first). `clear_remote_write` is async and cannot run from `Drop`, so both sites capture the result of the disk work and clear unconditionally before propagating the error. The mark is a plain `Mutex<HashSet<RelativePath>>`; the per-entry inflight lock already serializes same-path remote writes, so no refcounting is needed. Since platform remove notifications may arrive after the mark is cleared, `EntryManager::remove_entry` must treat an already-tombstoned row as a no-op and must not bump the local axis or rebroadcast metadata.

### Peer identity is currently untrusted

`source_id` on the TCP frame is read off the wire and **not** verified — there is no TLS or signature today. A follow-up will replace this with mutual TLS or Noise IK. Until that lands, any code that decides "is this peer allowed to do X" cannot trust `source_id` for cross-peer authorization — only use it for routing.

As a cheap honest-collision guard, `TransportReceiver::handle_handshake` rejects any handshake whose `source_id == local_id` and logs loudly: a peer declaring our own device id means a duplicated `device_id` (config copy, restored backup, baked-in container id), and without the guard both sides fall through the `local_id < peer_id` tiebreak in `handle_conflict` and overwrite each other. This guard does **not** defend against a malicious peer forging `source_id` — that still requires the cryptographic identity work.

### Runtime / data files (not in repo)

State lives in the OS config dir (`dirs::config_dir()` + `synche/`), not the repo. The paths are resolved through a `SyncheDirs` value type (`app/src/utils/dirs.rs`) carried on `AppState`, **not** through global statics — tests rely on injecting per-test `SyncheDirs` for isolation. Production code goes through `AppState::new_from_os()` and reads paths via `state.dirs()`. Don't reintroduce global `OnceLock`s for these directories.

- `config.toml` — `home_path` and the list of sync directories. Auto-generated on first run. Edits applied live; a `home_path` change triggers the restart loop above.
- `data.db` — SQLite store for entry metadata (`SqliteDb`).
- device-id file — persistent UUID for this device. A fresh `instance_id` is generated per process start; `local_id` persists.

### Test isolation

`#[tokio::test]`s run in parallel, so every test that needs an `AppState` MUST build one through `crate::utils::test_support::test_env()` (or `test_env_with_dirs`). The helper gives each test:

- A unique `TempDir` rooted in `/tmp`.
- A `SyncheDirs` rooted inside that temp dir — fresh `device_id` and `config.toml` per test.
- A seeded `config.toml` so `Config::init` does not touch the real `~/.config/synche/`.
- An `AppState` with ephemeral ports (`http: 0, presence: 0, transport: 0`).

Never construct a production `AppState` from a test (the binary builds it via `Synchronizer::run_default_with_restart` from a `SyncheDirs::from_os()` resolved in `main`), never `TempDir::new_in(state.home_path())` against the real home, never write to `./` or any other CWD-relative path. Hold the returned `TestEnv` (or its `_env` binding) for the lifetime of the test so the temp dir doesn't drop early.

### Frontend

`gui/index.html` is a single-page UI rendered via `minijinja` and served by axum; static assets in `gui/static/`. The server pushes live updates to the GUI over SSE using `ServerEvent` broadcast through `AppState::sse_sender()`. Variants currently include peer connect/disconnect, sync-directory add/remove, `ServerRestart`, and the per-entry `EntrySyncStarted` / `EntrySyncCompleted` / `EntrySyncFailed` events broadcast from the transport receiver path so the GUI can show live per-directory sync activity.

The peer and sync-directory lists include template-rendered `.empty-state` rows whose visibility is maintained by `gui/static/components.js` after API/SSE list updates. Keep the empty-state copy, CSS design tokens, focus styles, and the GUI contract tests in `infra/http/gui.rs` in sync when changing the visual shell.

The GUI visual shell is a polished dashboard: `.app-shell` contains a persistent `.brand-rail` for logo, local device identity, settings, version, and source link, plus a `.dashboard` content area for directories, devices, and conflicts. The palette is intentionally multi-hue and anchored on the logo green `#04745c`: green is for brand/healthy sync states, indigo for primary actions, cyan for network/devices, amber for activity/warnings, and red for destructive/conflict states. Preserve light/dark token pairs, high-contrast text, visible focus states, and responsive rules when changing the UI; do not collapse the app back to a monochromatic green theme or a card-style top header.

All GUI `/api/*` calls that can fail must go through the shared feedback helper in `gui/static/api_feedback.js` (or an equivalent central wrapper if it is replaced). Non-success HTTP responses must surface the status plus a short user-readable reason, network failures must show `Could not reach Synche.`, and modal forms must stay open until the action succeeds so the user can correct input.

### App version

The crate version (`env!("CARGO_PKG_VERSION")`, from `app/Cargo.toml`) is the single source of truth and is surfaced at runtime in five places: the startup log line in `main.rs`, the `version` field on the root `synche` tracing span, the GUI footer (via the `version` template variable in `infra/http/gui.rs`), the `X-Synche-Version` response header inserted by the middleware in `infra/http/server.rs`, and the `GET /api/info` endpoint in `infra/http/api.rs`. Never introduce a second source — always read through `env!`.

### Logging

Subscriber wired in `main.rs` via `utils::logging::init(dirs.log_dir())`. Returns a `LogGuards` that **must outlive `main`** — dropping it discards in-flight log lines from the non-blocking file appender.

- Output: stdout (ANSI when stdout is a TTY, plain text when piped/redirected, no target) **and** a daily-rotated file at `<log dir>/synche.log.<date>` (no ANSI, target included). Default log dirs: Linux `~/.local/state/synche/` (or `$XDG_STATE_HOME/synche`), macOS `~/Library/Logs/synche/`, Windows `%LOCALAPPDATA%\synche\logs\`. The appender keeps the last 14 daily files and prunes older ones on rotation.
- Default level: `synche=debug,warn` in debug builds, `synche=info,warn` in release.
- Override at runtime with `RUST_LOG` (standard `tracing_subscriber::EnvFilter` syntax), e.g. `RUST_LOG=synche=trace cargo run -p synche`.
- Log lines pick up context from spans rather than message bodies — prefer `#[tracing::instrument(skip_all, fields(peer = %id, entry = %name))]` on per-peer/per-entry handlers, then keep the message itself short. Root span is `synche{device, instance}` on `Synchronizer::_run`. HTTP requests are spanned by `tower_http::trace::TraceLayer`.
- No emojis in log messages.
