# Build and Run Synche from source

This guide provides instructions on how to build, run, and configure **Synche** from source.

> [!IMPORTANT]
> The `main` branch is used for active development and may be unstable. For a stable version, please use one of the [Release branches](https://github.com/matx64/synche/releases).

## Prerequisites

To build Synche from source, you will need:

- [Rust](https://www.rust-lang.org/tools/install) (version 1.80 or later recommended)
- [Git](https://git-scm.com/downloads)

Synche is cross-platform and supports Linux, macOS, and Windows.

## Build Instructions

First, clone the repository and navigate into the project directory. Then, build the project in release mode for optimal performance.

```sh
git clone https://github.com/matx64/synche.git
cd synche
cargo build --release
```

The compiled binary will be available at `target/release/synche`.

## Running Synche

To run the application, execute the binary from the project's root directory:

```sh
./target/release/synche
```

Once running, you can access the web interface at **`http://localhost:42880`** to configure your synchronized directories and monitor connected devices.

## Configuration

The first time you run Synche, it will automatically generate a `config.toml` file to store its configuration. This file is located in the standard configuration directory for your operating system:

- **Linux**: `$XDG_CONFIG_HOME/synche` or `$HOME/.config/synche`
- **macOS**: `$HOME/Library/Application Support/synche`
- **Windows**: `%APPDATA%\synche`

You can customize the following settings in `config.toml`:

### `config.toml` Example

Here is an example configuration file with explanations for each setting.

```toml
# A unique ID for this device. This is generated automatically and should not be
# copied between different devices.
device_id = "88bd9d3e-6c27-471f-a4d1-07446f0f3a1f"

# The root directory where your synchronized folders will be stored.
# If not specified, Synche defaults to:
# - Unix: $HOME/Synche
# - Windows: C:\Users\<User>\Synche
home_path = "/home/matx/Synche"

# A list of directories to synchronize. Each directory listed here will be created
# inside the `home_path` and synchronized with other devices that have a directory
# with the same name.
[[directory]]
name = "Default Folder"

[[directory]]
name = "A tiny Project"

# Network ports used by Synche for the web UI, device discovery, and data transfer.
# You can change these if they conflict with other services.
[ports]
http = 42880      # Port for the Web GUI
presence = 42881  # Port for device discovery on the local network
transport = 42882 # Port for encrypted data synchronization
```

**Note:** If you make changes to `config.toml`, you must restart the Synche application for the new settings to take effect.

## Feedback

Your feedback is very important at this stage. If you encounter issues or unexpected behavior, please [open an issue](https://github.com/matx64/synche/issues).
