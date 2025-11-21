![logo.png](https://i.postimg.cc/mDzfDtcj/logo.png)

---

**Synche** is an open-source, peer-to-peer file synchronization tool that operates entirely on your local network. It automatically syncs files between your devices, similar to Dropbox or Syncthing, but without requiring any cloud services or external servers.

## Features

-   **Local-Only:** No internet or cloud dependency.
-   **Automatic Discovery:** Devices running Synche on the same network find each other automatically using mDNS.
-   **.gitignore Support:** Respects your `.gitignore` files, perfect for syncing source code.
-   **Real-Time Sync:** Uses a file watcher to detect changes and synchronize them instantly.
-   **Peer-to-Peer:** Files are transferred directly between your devices.
-   **Web Interface:** A simple, browser-based GUI for managing the app.

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

You can either download a prebuilt binary or build it from source:

-   **[Latest Release](https://github.com/matx64/synche/releases)** (Recommended for most users)
-   **[Build Guide](docs/BUILD.md)** (For developers who want to build from source)

> [!TIP]
> Check out the **[Practical Example](docs/EXAMPLE.md)** to learn how to synchronize your first folder between two devices.

## Roadmap

-   [x] Local network device discovery (mDNS)
-   [x] File watcher and P2P sync over TCP
-   [x] Version vectors for conflict resolution and integrity checks
-   [x] SQLite persistence for metadata
-   [x] `.gitignore` support
-   [x] Web GUI
-   [ ] Transfer file blocks instead of the whole file
-   [ ] Performance and resource optimization

## Contributing & Feedback

This project is in active development, and contributions are welcome. If you find a bug, have a feature request, or want to contribute, please [**open an issue**](https://github.com/matx64/synche/issues) or submit a pull request.

## License

Copyright © 2025-present, [Synche Contributors](https://github.com/matx64/synche/graphs/contributors).

This project is licensed under the [MIT License](https://github.com/matx64/synche/blob/main/LICENSE).
