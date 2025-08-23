![logo.png](https://i.postimg.cc/mDzfDtcj/logo.png)

---

> **TL;DR:** A lightweight & local-only alternative to [Syncthing](https://syncthing.net/).

**Synche** is an open source p2p **continuous file synchronization tool** for devices on the same local network. It watches, syncronizes folders & files just like Google Drive/Syncthing without the need for cloud services or external servers.

## Features

- **Local-only operation** (no internet/cloud dependency)
- **Automatic device discovery** on local network
- **.gitignore** support
- **Continuous file monitoring**
- **Peer-to-peer file synchronization**
- **Minimal Configuration**
- **Memory safe**
- ❌ Native GUI frontend (coming soon)

## Use Cases

Synche was primarily _**created for developers**_ to sync source code automatically between computers without the need to commit + push to a remote repo, that's why .gitignore support was a requirement. However, it can also be used for offline backup, share media and IoT.

## How it works

1. Devices on the same network discover each other via mDNS Service Discovery.
2. Each device chooses the root folders to synchronize and watches for file changes.
3. Changes are propagated to connected peers in real-time using TCP.
4. File versoning is handled using version vectors and **conflicts are resolved by the user** to ensure data safety.

## Roadmap

- [x] Local network device discovery (mDNS)
- [x] File/Folders watching and sync over TCP
- [x] Version Vectors implementation
- [x] File integrity checks
- [x] Persistent filesystem state
- [x] Support .gitignore
- [x] Testing & stability improvements before 0.0.1
- [ ] Cross-platform 0.0.1 builds
- [ ] Performance Optimizations
- [ ] File blocks implementation
- [ ] Native GUI frontend (desktop)

## License

Copyright © 2025-present, [Synche Contributors](https://github.com/matx64/synche/graphs/contributors).

This project is [MIT](https://github.com/matx64/synche/blob/main/LICENSE) licensed.
