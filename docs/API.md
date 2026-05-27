# HTTP API Reference

Synche exposes a small HTTP API for managing sync directories and streaming real-time events to the GUI.  The server runs on port **42880** by default (configurable in `AppState`).

All requests use query parameters — there are no JSON request bodies.  Responses carry only an HTTP status code; error details are emitted to the server log.

> **Source:** [`app/src/infra/http/api.rs`](../app/src/infra/http/api.rs) · [`app/src/domain/sse.rs`](../app/src/domain/sse.rs)

---

## Endpoints

### `GET /api/events` — Server-Sent Events stream

Streams [`ServerEvent`](#server-sent-events) objects to the client as newline-delimited JSON using the SSE protocol.  The GUI subscribes to this endpoint on load and uses it to update its peer list and sync-directory list in real time.

| | |
|---|---|
| **Method** | `GET` |
| **Query params** | none |
| **Content-Type** | `text/event-stream` |
| **Status** | `200 OK` (stream never ends until the server restarts or the broadcast channel closes) |

**Behaviour:**

- Each event is serialized with `serde_json` and delivered as `data: <json>\n\n`.
- The broadcast channel has a capacity of **100 events**.  If a client falls behind, it is warned in the log and continues receiving subsequent messages (intermediate events are skipped).
- When the broadcast channel closes (e.g. after a clean shutdown), the stream ends.
- A `ServerRestart` event is sent before any in-process restart so the client knows to reconnect.

---

### `POST /api/add-sync-dir` — Add a sync directory

Appends a directory to the sync configuration.

| | |
|---|---|
| **Method** | `POST` |
| **Query params** | `name` — directory name relative to `home_path` (leading/trailing whitespace is trimmed) |
| **Request body** | none |

| Status | Meaning |
|--------|---------|
| `201 Created` | Directory was added successfully |
| `409 Conflict` | Directory is already present in the configuration |
| `500 Internal Server Error` | Unexpected I/O error writing `config.toml` |

**Example:**

```
POST /api/add-sync-dir?name=Photos
```

---

### `POST /api/remove-sync-dir` — Remove a sync directory

Removes a directory from the sync configuration.  The operation is idempotent — removing a directory that does not exist in the config returns `200 OK`.

| | |
|---|---|
| **Method** | `POST` |
| **Query params** | `name` — directory name relative to `home_path` (leading/trailing whitespace is trimmed) |
| **Request body** | none |

| Status | Meaning |
|--------|---------|
| `200 OK` | Directory removed (or was not present) |
| `500 Internal Server Error` | Unexpected I/O error writing `config.toml` |

**Example:**

```
POST /api/remove-sync-dir?name=Archive
```

---

### `POST /api/set-home-path` — Change the home directory

Updates the root directory used as the base for all sync paths.  If the directory does not yet exist it (and any missing parents) is created.  Changing this value triggers an in-process restart of the synchronizer — see the [restart sentinel contract](ARCHITECTURE.md#home_path-change--restart-sentinel-contract) for details.

| | |
|---|---|
| **Method** | `POST` |
| **Query params** | `path` — absolute filesystem path |
| **Request body** | none |

| Status | Meaning |
|--------|---------|
| `200 OK` | Home path updated and written to `config.toml` |
| `400 Bad Request` | `path` exists but is a file rather than a directory |
| `500 Internal Server Error` | Other I/O error (e.g. permission denied) |

**Example:**

```
POST /api/set-home-path?path=/home/user/synced-data
```

---

## GUI routes

These routes are served by the same HTTP server but are not part of the JSON API.

### `GET /` — Web GUI

Renders the Synche single-page UI as an HTML page.  The template receives the current application state as template variables:

| Variable | Type | Description |
|----------|------|-------------|
| `dirs` | list of strings | Currently configured sync directory names |
| `hostname` | string | Local machine hostname |
| `local_id` | UUID string | Persistent device identifier |
| `peers` | list of peer objects | Currently connected peers |
| `local_ip` | IP address string | Local network IP address |
| `home_path` | string | Absolute path of the current home directory |

| Status | Meaning |
|--------|---------|
| `200 OK` | Page rendered successfully |
| `500 Internal Server Error` | Template rendering failed |

### `GET /static/*` — Static assets

Serves CSS, JavaScript, and other static files from `gui/static/`.  Returns `404 Not Found` if the requested file does not exist.

---

## Server-Sent Events

All events are JSON-serialized variants of the `ServerEvent` enum ([`app/src/domain/sse.rs`](../app/src/domain/sse.rs)).

### `PeerConnected`

A new peer was discovered on the network, or an existing peer reconnected.

```json
{
  "PeerConnected": {
    "id": "550e8400-e29b-41d4-a716-446655440000",
    "addr": "192.168.1.42",
    "hostname": "laptop",
    "instance_id": "7b3f9c1a-2d4e-4f5a-b6c7-d8e9f0a1b2c3",
    "last_seen": 1748390400,
    "sync_dirs": ["Documents", "Photos"]
  }
}
```

| Field | Type | Description |
|-------|------|-------------|
| `id` | UUID string | Persistent device identifier (`local_id`) |
| `addr` | IP address string | IPv4 address of the peer |
| `hostname` | string | Peer hostname (`.local` suffix stripped) |
| `instance_id` | UUID string | Regenerated on every process start; a change signals a peer restart |
| `last_seen` | integer | UNIX timestamp (seconds) of the peer's most recent presence announcement |
| `sync_dirs` | array of strings | Names of the sync directories this peer is sharing (relative to its home path) |

### `PeerDisconnected`

A peer timed out or was explicitly evicted.

```json
{
  "PeerDisconnected": "550e8400-e29b-41d4-a716-446655440000"
}
```

The inner value is the UUID (`local_id`) of the disconnected peer.

### `SyncDirectoryAdded`

A sync directory was added to the local configuration.

```json
{
  "SyncDirectoryAdded": "Photos"
}
```

The inner value is the directory name relative to `home_path`.

### `SyncDirectoryRemoved`

A sync directory was removed from the local configuration.

```json
{
  "SyncDirectoryRemoved": "Archive"
}
```

The inner value is the directory name relative to `home_path`.

### `ServerRestart`

The server is about to perform an in-process restart (e.g. after a `home_path` change).  Clients should reconnect to `/api/events` after receiving this event.

```json
{
  "ServerRestart": null
}
```

---

## Data types

### `RelativePath`

A string path relative to `home_path`.  Always uses forward slashes `/` as separators regardless of the host OS.  Validated to exclude absolute paths, `..` traversal components, empty strings, and backslash-separated paths received from peers.

### `config.toml` format

The persistent configuration written by the API endpoints:

```toml
home_path = "/path/to/sync/home"

[[directory]]
name = "Photos"

[[directory]]
name = "Documents"
```

---

## Route summary

| Route | Method | Query params | Status codes |
|-------|--------|--------------|-------------|
| `/api/events` | GET | — | 200 (SSE stream) |
| `/api/add-sync-dir` | POST | `name` | 201, 409, 500 |
| `/api/remove-sync-dir` | POST | `name` | 200, 500 |
| `/api/set-home-path` | POST | `path` | 200, 400, 500 |
| `/` | GET | — | 200, 500 |
| `/static/*` | GET | — | 200, 404 |
