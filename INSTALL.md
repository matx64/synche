# Installation Guide

This document explains how to build and run **Synche** from source. As the project is still in alpha, there's no pre-built binaries.

> ⚠️ **ALWAYS use a Release [(latest)](https://github.com/matx64/synche/releases/latest) and its INSTALL.md file, `main` branch is currently being used for development.**

## Prerequisites

- [Rust](https://www.rust-lang.org/tools/install) (version 1.80+ recommended)
- Git

By default, Synche uses ports 42880 (http), 42881 (presence/mDNS) and 42882 (TCP), so make sure these ports are allowed by your OS Firewall. It shouldn't be an issue, but it is **recommended** to guarantee mDNS service is allowed for **MacOS** by executing these commands:

```sh
# allow mDNS in MacOS
sudo /usr/libexec/ApplicationFirewall/socketfilterfw --add /usr/libexec/mdnsd
sudo /usr/libexec/ApplicationFirewall/socketfilterfw --unblock /usr/libexec/mdnsd
```

## Build

Download the [latest release](https://github.com/matx64/synche/releases/latest) and extract its contents.

```sh
cd release-folder
cargo build --release
```

The compiled binary (executable) will be located at: `target/release/synche`.

## Run

Execution command:

```sh
./target/release/synche
```

You can now configure which folders you want to sync across devices using the `.synche/config.toml` file (same location as the executable).

Make sure to add the same folders in the other devices config file as well and to restart Synche on every config change. Pattern to follow:

```toml
device_id = "88bd9d3e-6c27-471f-a4d1-07446f0f3a1f"
home_path = "/home/matx/dev/synche/Synche"

[[directory]]
name = "Default Folder"

[[directory]]
name = "A tiny Project"

[ports]
http = 42880
presence = 42881
transport = 42882
```

Synced entries will reside in `Synche` folder.

## Practical Example

Synche is running in my laptop and desktop computers with the same `directory` in `config.toml` file:

```toml
[[directory]]
name = "synche-git-repo"

[[directory]]
name = "project001"

# Device specific settings
# device_id = ...
# home_path = ...

# [ports]
# ...

```

I modify the `Synche/synche-git-repo/INSTALL.md` file in the laptop. This change must be propagated to the desktop automatically.

## Feedback

Your feedback is very important at this stage. If you encounter issues or unexpected behavior, please [open an issue](https://github.com/matx64/synche/issues).
