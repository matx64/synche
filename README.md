<h1>
  <p align="center">
    <picture>
      <source
        media="(prefers-color-scheme: dark)"
        srcset="/gui/static/icon.svg"
      />
      <source
        media="(prefers-color-scheme: light)"
        srcset="/gui/static/icon-dark.svg"
      />
      <img alt="Synche logo" src="/gui/static/icon-dark.svg" width="128" />
    </picture>
    <br>Synche
  </p>
</h1>

**Synche** is an open-source, peer-to-peer file synchronization tool that operates entirely on your local network. It automatically syncs files between your devices, similar to Dropbox or Syncthing, but without requiring any cloud services or external servers.

<picture>
  <source media="(prefers-color-scheme: dark)" srcset="https://github.com/matx64/synche/releases/download/v0.0.6-alpha/ss-dark.png" />
  <source media="(prefers-color-scheme: light)" srcset="https://github.com/matx64/synche/releases/download/v0.0.6-alpha/ss-light.png" />
  <img alt="Synche GUI screenshot" src="https://github.com/matx64/synche/releases/download/v0.0.6-alpha/ss-light.png" />
</picture>

## Features

-   **Local-Only:** No internet or cloud dependency.
-   **Automatic Discovery:** Devices running Synche on the same network find each other automatically using mDNS.
-   **.gitignore Support:** Respects your `.gitignore` files, plus `.git/` directories are always excluded — safe to sync folders containing Git repositories.
-   **Real-Time Sync:** Uses a file watcher to detect changes and synchronize them instantly.
-   **Live Activity Feedback:** The web GUI shows per-directory sync activity as files are received from peers, including the most recent completed and failed transfers.
-   **Conflict Resolution in the GUI:** When concurrent edits create a conflict copy, the web GUI lists it under a **Conflicts** section and lets you resolve it with one click — keep the current file or adopt the conflict copy — with the change propagated to all peers.
-   **Peer-to-Peer:** Files are transferred directly between your devices.
-   **Web Interface:** A simple, browser-based GUI for managing the app.
-   **Configurable Ports & CLI Flags:** Override the HTTP/presence/transport ports via a `[ports]` block in `config.toml` or `--http-port`/`--presence-port`/`--transport-port` flags, and relocate all state with `--config-dir` to run multiple instances on one host.

## Why Synche?

Synche was primarily built for developers to keep source code synchronized across multiple computers without the friction of frequent `git commit + push`. However, it can also be used for offline backups, share media and IoT.

## How it works

1.  **Discovery:** Devices on the same local network discover each other using mDNS.
2.  **Watching:** Synche monitors your specified folders for any file or directory changes.
3.  **Synchronization:** When a change is detected, its metadata is announced to all peers. The data is then transferred directly over TCP to any peer that needs the update.
4.  **Conflict Resolution:** To prevent data loss, Synche uses version vectors to track file history. If a file is modified on multiple devices simultaneously, a conflict file is created, allowing you to resolve the conflict manually.

## Getting Started

> [!NOTE]
> Synche is currently in alpha. It is functional but may still contain bugs. Please avoid using it with critical data.
>
> Also, check out the **[Practical Example](docs/EXAMPLE.md)** to learn how to synchronize your first folder between two devices.

You can either download a prebuilt binary or build it from source:

-   **[Latest Release](https://github.com/matx64/synche/releases/latest)** (Recommended for most users)
-   **[Build Guide](docs/BUILD.md)** (For developers who want to build from source)

## Documentation

-   **[HTTP API Reference](docs/API.md)** — endpoint details, query parameters, response codes, and SSE event shapes.
-   **[Architecture Guide](docs/ARCHITECTURE.md)** — hexagonal layout, version vectors and conflict resolution, TCP wire format, `home_path` restart contract, and mDNS discovery.

## Contributing & Feedback

This project is in active development, and contributions are welcome. Check out the **[Contributing Guide](docs/CONTRIBUTING.md)** for more details.

## License

Copyright © 2025-present, [Synche Contributors](https://github.com/matx64/synche/graphs/contributors).

This project is licensed under the [MIT License](https://github.com/matx64/synche/blob/main/LICENSE).
