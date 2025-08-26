![logo.png](https://i.postimg.cc/mDzfDtcj/logo.png)

---

> **TL;DR:** A lightweight & local-only alternative to [Syncthing](https://syncthing.net/).

**Synche** is an open source peer-to-peer **continuous file synchronization tool** for devices on the same local network. It watches and syncronizes files just like Dropbox/Syncthing without the need for cloud services or external servers.

## Features

- **Local-only operation** (no internet/cloud dependency)
- **Automatic device discovery** on local network
- **.gitignore** support
- **Continuous file monitoring**
- **P2P file synchronization**
- **Minimal Configuration**
- **Memory safe**
- ❌ Native GUI frontend (coming soon)

## Use Cases

Synche was primarily _**created for developers**_ to sync source code automatically between computers without the need to commit + push to a remote repo, that's why .gitignore support was a requirement. However, it can also be used for offline backup, share media and IoT.

## How it works

1. Devices on the same network discover each other via mDNS Service Discovery.
2. Each device chooses the root folders to synchronize and watches for file/folder changes.
3. Changes are propagated to connected peers in real-time using TCP.
4. Version vectors are tracked and **conflicts are resolved by the user** by creating a conflict file to ensure data safety.

## Try it out!

Synche is currently in alpha. It is functional but may contain bugs, so avoid using it with critical files. You can try it out by following the [Installation Guide](https://github.com/matx64/synche/blob/main/INSTALL.md).

## Roadmap

- [x] Local network device discovery (mDNS)
- [x] File watcher and Sync over TCP
- [x] Version vectors, Conflict resolution and Integrity checks
- [x] Sqlite Persistence
- [x] Support .gitignore
- [x] Release 0.0.1-alpha
- [ ] Advanced Network state checks
- [ ] File blocks implementation
- [ ] Native GUI frontend (desktop)

## License

Copyright © 2025-present, [Synche Contributors](https://github.com/matx64/synche/graphs/contributors).

This project is [MIT](https://github.com/matx64/synche/blob/main/LICENSE) licensed.
