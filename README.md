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
>
> Also, check out the **[Practical Example](docs/EXAMPLE.md)** to learn how to synchronize your first folder between two devices.

You can either download a prebuilt binary or build it from source:

-   **[Latest Release](https://github.com/matx64/synche/releases/latest)** (Recommended for most users)
-   **[Build Guide](docs/BUILD.md)** (For developers who want to build from source)

## Contributing & Feedback

This project is in active development, and contributions are welcome. Check out the **[Contributing Guide](docs/CONTRIBUTING.md)** for more details.

## License

Copyright © 2025-present, [Synche Contributors](https://github.com/matx64/synche/graphs/contributors).

This project is licensed under the [MIT License](https://github.com/matx64/synche/blob/main/LICENSE).
