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

**Synche** is an open-source, peer-to-peer file synchronization tool that runs entirely on your local network — like Dropbox or Syncthing, but with no cloud, servers, or accounts.

<picture>
  <source media="(prefers-color-scheme: dark)" srcset="https://github.com/matx64/synche/releases/download/v0.0.6-alpha/ss-dark.png" />
  <source media="(prefers-color-scheme: light)" srcset="https://github.com/matx64/synche/releases/download/v0.0.6-alpha/ss-light.png" />
  <img alt="Synche GUI screenshot" src="https://github.com/matx64/synche/releases/download/v0.0.6-alpha/ss-light.png" />
</picture>

## Features

-   **Local & peer-to-peer** — no cloud or internet dependency; files transfer directly between devices.
-   **Auto-discovery** — peers on the same network find each other over mDNS.
-   **Real-time sync** — a file watcher detects changes and propagates them instantly over TCP.
-   **Web GUI** — manage sync directories and watch live per-directory activity from the browser.
-   **Git aware** — respects `.gitignore` and always excludes `.git/`, so syncing repos is safe.
-   **Conflict resolution** — detect concurrent edits and create conflict copies you resolve in the GUI.

## Getting Started

> [!NOTE]
> Synche is currently in alpha. It is fully functional but may contain bugs. Please avoid using it with critical data.
>
> Also, check out the **[Practical Example](docs/EXAMPLE.md)** to learn how to synchronize your first folder between two devices.

You can either download a prebuilt binary or build it from source:

-   **[Latest Release](https://github.com/matx64/synche/releases/latest)** (Recommended for most users)
-   **[Build Guide](docs/BUILD.md)** (For developers who want to build from source)

## Documentation & Contributing

-   **[Architecture Guide](docs/ARCHITECTURE.md)** — how Synche works under the hood.
-   **[HTTP API Reference](docs/API.md)** — endpoints and events.

This project is in active development, and contributions are welcome — see the **[Contributing Guide](docs/CONTRIBUTING.md)**.
