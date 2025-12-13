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

Once running, you can access the web interface at **`http://localhost:42880`** to configure your synchronized directories, home path and monitor connected devices.

## Configuration

The first time you run Synche, it will automatically generate a `config.toml` file. This file is located in the standard configuration directory for your operating system:

- **Linux**: `$XDG_CONFIG_HOME/synche` or `$HOME/.config/synche`
- **macOS**: `$HOME/Library/Application Support/synche`
- **Windows**: `%APPDATA%\synche`

### `config.toml` Example

Here is an example configuration file with explanations for each setting.

```toml
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
```

**Note:** Changes are applied automatically without having to manually restart Synche. 

## Feedback

Your feedback is very important at this stage. If you encounter issues or unexpected behavior, please [open an issue](https://github.com/matx64/synche/issues).
