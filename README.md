![logo.png](https://i.postimg.cc/mDzfDtcj/logo.png)

---

> **TL;DR:** A lightweight & local-only alternative to [Syncthing](https://syncthing.net/).

**Synche** is a p2p continuous file synchronization tool for devices on the same local network. It enables fast, private, and automatic syncing without the need for cloud services or external servers.

## 🚀 Features

- ✅ **Local-only operation** (no internet/cloud dependency)
- ✅ **Automatic device discovery** on local network
- ✅ **Continuous file monitoring** using efficient file watchers
- ✅ **Peer-to-peer file synchronization**
- ✅ **Memory safe**
- 🚧 **.gitignore** support (under development)
- ❌ Native GUI frontend (coming soon)

## 🔧 How it works

1. Devices on the same network discover each other via mDNS Service Discovery.
2. Each device chooses the root folders to synchronize and watches for file changes.
3. Changes are propagated to connected peers in real-time using TCP.
4. File versions are handled using Version Vectors and **conflicts are resolved by the user** to ensure data safety.

## 📦 Roadmap

- [x] Local network device discovery (mDNS)
- [x] File/Folders watching and sync over TCP
- [x] Version Vectors implementation
- [x] File integrity checks
- [x] Persistent filesystem state
- [ ] Support .gitignore
- [ ] Testing & stability improvements
- [ ] Cross-platform Alpha builds
- [ ] Performance Optimizations
- [ ] File blocks implementation
- [ ] Native GUI frontend (desktop)
