# Installation Guide

This document explains how to build and run **Synche** from source. As the project is still in alpha, there's no pre-built binaries.

## Prerequisites

- [Rust](https://www.rust-lang.org/tools/install) (version 1.80+ recommended)
- Git

Synche uses ports 5200 (mDNS) and 8889 (TCP), so make sure these ports are allowed by your OS Firewall. It shouldn't be an issue, but it is **recommended** to guarantee mDNS service is allowed for **MacOS** by executing these commands:

```sh
# allow mDNS in MacOS
sudo /usr/libexec/ApplicationFirewall/socketfilterfw --add /usr/libexec/mdnsd
sudo /usr/libexec/ApplicationFirewall/socketfilterfw --unblock /usr/libexec/mdnsd
```

## Build

```sh
git clone https://github.com/matx64/synche.git
cd synche

cargo build --release
```

The compiled binary (executable) will be located at: `target/release/synche`.

## Run

Execution command:

```sh
./target/release/synche
```

You can now configure which folders you want to sync across devices using the `.synche/config.json` file (same location as the executable).

Make sure to add the same folders in the other devices config file as well and to restart Synche on every config change. Pattern to follow:

```json
[{ "folder_name": "myfolder" }, { "folder_name": "project001" }]
```

Synced entries will reside in `synche-files` folder.

## Practical Example

Synche is running in my laptop and desktop computers with the same `config.json` file:

```json
[{ "folder_name": "synche-git-repo" }]
```

I modify the `synche-files/synche-git-repo/INSTALL.md` file in the laptop. This change must be propagated to the desktop automatically.

## Feedback

Your feedback is very important at this stage. If you encounter issues or unexpected behavior, please [open an issue](https://github.com/matx64/synche/issues).
