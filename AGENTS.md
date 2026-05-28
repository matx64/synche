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

Running the binary serves the web GUI at `http://localhost:42880`. Ports used: HTTP `42880`, presence (mDNS) `42881`, transport (TCP) `42882` — defined in `app/src/application/state/app_state.rs`.

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

## Architecture

Single Cargo workspace (root `Cargo.toml`) with one member crate at `app/`. Rust source lives under `app/src/` and follows a **hexagonal / ports-and-adapters** layout. Read this section before navigating individual files — the layer boundaries matter.

### Layers

- **`domain/`** — pure types, no I/O, no async. The full domain surface is re-exported from `app/src/domain/mod.rs`: `Config`, `Peer`, `EntryInfo`/`EntryKind`, `VersionVector`/`VersionCmp`, `CanonicalPath`/`RelativePath`, `SyncDirectory`, `AppPorts`, `ServerEvent`, the `Transport*` family, and channel helpers (`BroadcastChannel`, `MutexChannel`).
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

### Conflict resolution

`VersionVector = HashMap<Uuid, u64>` keyed by device `local_id` (`app/src/domain/entry/version.rs`). Comparing two versions yields `VersionCmp::{Equal, KeepSelf, KeepOther, Conflict}` — concurrent edits produce `Conflict` (which the system materializes as a conflict file) rather than overwriting. Anything that mutates `EntryInfo` or decides which side wins must go through this comparison.

### Permanent exclusions

Permanent path exclusions must be enforced at every boundary where entries can enter or leave sync: filesystem scans, watcher events, handshakes, metadata handling, request handling, transfer handling, and disk writes. Use `utils::fs::is_git_path` as the shared predicate for `.git/` path exclusion. It matches an exact `.git` path component only, so `.gitignore`, `.gitattributes`, `.github/`, and `foo.git/` remain syncable.

Remote transport paths must be validated before any metadata handling or disk write. Use `RelativePath::is_safe_sync_path` to reject absolute paths, parent-directory traversal, empty paths, and backslash-separated paths from peers.

### Scoping inbound entries to configured sync_dirs

Every inbound `Metadata`/`Request`/`Transfer` handler in `TransportReceiver` (`app/src/application/network/transport/receiver.rs`) drops entries whose top component is not a configured sync directory. The guard is `TransportReceiver::is_in_configured_sync_dir`, which delegates to `AppState::contains_sync_dir`. The same check already runs in `EntryManager::get_entries_to_request` and `build_db`. `TcpReceiver` must also enforce the configured-sync-dir guard before staging or finalizing inbound `Transfer` bytes, because application-layer filtering happens after the TCP adapter has decoded the frame. If you add a new inbound entry path, add the guard alongside the existing `is_git_path` filter — they belong together.

"Path under sync dir" checks must be component-aware: use `RelativePath::starts_with_dir`, never `str::starts_with` on a `RelativePath`. The dereference-to-`str` makes it look right, but `foo` would then match `foobar/...`.

### Inbound TCP message size caps

`app/src/infra/network/tcp/chunk.rs` defines three hard caps that are enforced **before** allocating: `MAX_TRANSFER_SIZE` (raw file bytes), `MAX_HANDSHAKE_JSON_SIZE` (handshake JSON), `MAX_ENTRY_JSON_SIZE` (single `EntryInfo` JSON). Anything that decodes a peer-supplied `u32` length must check it against the right cap before `vec![0u8; len]`. Don't add a new variable-length frame without picking (or adding) a cap.

### Sanitizing peer-supplied version vectors

Any path that persists a peer-supplied `EntryInfo` must strip foreign axes and reject counters above `MAX_TRUSTED_COUNTER` first. The hardened paths are `EntryManager::merge_versions_and_insert` (on the `Equal | KeepSelf` branch of `compare_and_resolve_conflict`) and `EntryManager::insert_peer_entry` (used by `TransportReceiver::handle_transfer` via `commit_staged_transfer`, and by `create_received_dir` for accepted directory-create entries). When replacing an existing row with a peer entry, preserve the trusted existing version vector and merge only the sender's own inbound axis; never reset the local axis to zero after accepting a transfer. `TcpReceiver` must reject or drain-and-drop poisoned `Transfer` frames before staging bytes, since the file payload is materialized before metadata is persisted. Plain `insert_entry` is for trusted local writes only — never call it directly on a peer-supplied entry.

Comparison decisions must use the same sanitized peer view before calling `EntryInfo::compare`. Do not let foreign axes influence `handle_metadata`, handshake request selection, conflict resolution, delete decisions, or transfer requests; sanitize to the sender's own axis first, then persist through the hardened paths above.

### Conflict-resolved-as-KeepSelf must NOT merge the peer's axis

When `compare_and_resolve_conflict` sees `VersionCmp::Conflict` and `handle_conflict` returns `KeepSelf` (the local-id tiebreak made us the winner), do **not** call `merge_versions_and_insert`. Absorbing the peer's counter under an axis whose content we never integrated would make our vector dominate the peer's on the next exchange — and the peer would then silently overwrite its own edit with no conflict file on either side (issue #33 B2). The current code threads the raw compare result alongside the post-`handle_conflict` outcome so the merge runs only on `Equal` or a non-conflict `KeepSelf`. Preserve that invariant.

### Durable tombstones

`EntryManager::delete_and_update_entry` must persist the tombstone via `db.insert_or_replace_entry` (bumped local counter + `REMOVED_HASH`), not call `db.delete_entry`. `build_db`'s "file missing on disk" branch must keep rows for which `entry.is_removed()` is true so a tombstone survives restart and continues propagating to peers via the handshake entry list. Handshake reconciliation must apply `entry.is_removed()` before checking `entry.is_file()`; tombstones are file entries with the removal sentinel, and must never enqueue a file `Request`. Without this, a crash between the row-delete and metadata broadcast — or any late-joining peer — would silently re-sync the deleted file back from a peer that still has the live copy (issue #33 B3). Tombstone GC / retention is a deliberate follow-up.

### Pre-rename validation of inbound Transfers

`TcpReceiver` writes verified Transfer bytes to a per-transfer staging directory in the OS temp dir and returns a `StagedTransfer` RAII guard on the `TransportEvent`; it **does not** rename into `home_path`. The application layer commits via `EntryManager::commit_staged_transfer`, which runs four checks before the atomic rename + metadata persist (issue #33 B1):

1. `is_git_path` and `AppState::contains_sync_dir` (defense in depth — already enforced at the TCP layer pre-stage).
2. `AppState::take_pending_request(peer_id, name)` — every legitimate Transfer is preceded by a `Request` this device registered via `AppState::register_pending_request` in `handle_metadata` / `handle_handshake`. Unsolicited transfers are dropped without touching disk.
3. `local.compare(sanitized_peer)` must yield `Equal`, `KeepOther`, or `Conflict→KeepOther`. A `KeepSelf` outcome drops the staged bytes; the local edit is preserved.
4. `AppState::acquire_inflight_lock(name)` serializes concurrent commits of the same path. Always pair it with `release_inflight_lock` when done.

On any failure path the `StagedTransfer` is dropped and its `Drop` impl synchronously cleans up the staging directory. Do not bypass `commit_staged_transfer` to write Transfer bytes directly into `home_path`. If `fs::rename(staging, target)` reports `CrossesDevices`, copy to a temporary sibling inside the target directory and then rename that temp file to the final target; never copy directly over the user file.

### Peer identity is currently untrusted

`source_id` on the TCP frame is read off the wire and **not** verified — there is no TLS or signature today. The follow-up tracked under issue #32 will replace this with mutual TLS or Noise IK. Until that lands, any code that decides "is this peer allowed to do X" cannot trust `source_id` for cross-peer authorization — only use it for routing.

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

### App version

The crate version (`env!("CARGO_PKG_VERSION")`, from `app/Cargo.toml`) is the single source of truth and is surfaced at runtime in five places: the startup log line in `main.rs`, the `version` field on the root `synche` tracing span, the GUI footer (via the `version` template variable in `infra/http/gui.rs`), the `X-Synche-Version` response header inserted by the middleware in `infra/http/server.rs`, and the `GET /api/info` endpoint in `infra/http/api.rs`. Never introduce a second source — always read through `env!`.

### Logging

Subscriber wired in `main.rs` via `utils::logging::init(dirs.log_dir())`. Returns a `LogGuards` that **must outlive `main`** — dropping it discards in-flight log lines from the non-blocking file appender.

- Output: stdout (ANSI when stdout is a TTY, plain text when piped/redirected, no target) **and** a daily-rotated file at `<log dir>/synche.log.<date>` (no ANSI, target included). Default log dirs: Linux `~/.local/state/synche/` (or `$XDG_STATE_HOME/synche`), macOS `~/Library/Logs/synche/`, Windows `%LOCALAPPDATA%\synche\logs\`. The appender keeps the last 14 daily files and prunes older ones on rotation.
- Default level: `synche=debug,warn` in debug builds, `synche=info,warn` in release.
- Override at runtime with `RUST_LOG` (standard `tracing_subscriber::EnvFilter` syntax), e.g. `RUST_LOG=synche=trace cargo run -p synche`.
- Log lines pick up context from spans rather than message bodies — prefer `#[tracing::instrument(skip_all, fields(peer = %id, entry = %name))]` on per-peer/per-entry handlers, then keep the message itself short. Root span is `synche{device, instance}` on `Synchronizer::_run`. HTTP requests are spanned by `tower_http::trace::TraceLayer`.
- No emojis in log messages.
