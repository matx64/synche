# 🗃️ Synche

> **TL;DR:** A lightweight & local-only alternative to [Syncthing](https://syncthing.net/).

**Synche** is a continuous file synchronization tool for devices on the same local network. It enables fast, private, and automatic syncing without the need for cloud services or external servers.



## 🚀 Features

- ✅ **Local-only operation** (no internet/cloud dependency)
- ✅ **Automatic device discovery** on local network
- ✅ **Continuous file monitoring** using efficient file watchers
- ✅ **Peer-to-peer file synchronization**
- ❌ Native GUI frontend (coming soon)




## 🔧 How it works

1. Devices on the same network discover each other via UDP broadcast.
2. Each device chooses the files to synchronize and watches for file changes.
3. Changes are propagated to connected peers in real-time using TCP.
4. Conflicts are currently resolved by "last modified" timestamp (custom conflict resolution to be explored).




## 📦 Roadmap

- [x] Local network device discovery (UDP)
- [x] File watching and sync over TCP
- [x] Basic conflict resolution (last modified wins)
- [ ] Native GUI frontend (desktop)
- [ ] Selective file sync in GUI
- [ ] Folder watching and sync over TCP
- [ ] File integrity checks
- [ ] Cross-platform builds
- [ ] Testing & stability improvements