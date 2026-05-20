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

Never call `AppState::new_from_os()` from a test, never `TempDir::new_in(state.home_path())` against the real home, never write to `./` or any other CWD-relative path. Hold the returned `TestEnv` (or its `_env` binding) for the lifetime of the test so the temp dir doesn't drop early.

### Frontend

`gui/index.html` is a single-page UI rendered via `minijinja` and served by axum; static assets in `gui/static/`. The server pushes live updates to the GUI over SSE using `ServerEvent` broadcast through `AppState::sse_sender()`.
