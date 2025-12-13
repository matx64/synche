# Synche Installation

This guide covers the installation of **Synche** from a prebuilt binary.

## 1. Installation

1.  Download the latest release from the [**GitHub Releases page**](https://github.com/matx64/synche/releases/latest).
2.  Extract the archive to a permanent location (e.g., `~/synche`, `/Applications/synche`, `C:\Tools\Synche`).

> [!IMPORTANT]
> The `synche` executable requires the `gui/` directory to be present in the same location to serve the web client. Do not move the executable by itself.

3.  **(Optional)** To make the executable available system-wide, add it to your `PATH`.
    -   **Linux/macOS**: Create a symbolic link.
        ```sh
        # Adjust the path to where you extracted Synche
        sudo ln -s ~/synche /usr/local/bin/synche
        ```
    -   **Windows**: Add the Synche directory to your `Path` environment variable.

## 2. Running Synche

Start the application from your terminal:

```sh
synche
```

-   The Web GUI is available at **`http://localhost:42880`**.
-   Synche home directory is created by default at: `~/Synche` (Unix) or `C:\Users\<User>\Synche` (Windows).
-   Stop the process with `Ctrl+C`.

### Firewall

Ensure your firewall allows traffic on the ports: `42880` (HTTP), `42881` (Presence/mDNS), and `42882` (Transport/TCP).

## 3. Configuration

On the first run, a `config.toml` file is created in the standard OS configuration directory:

-   **Linux**: `$XDG_CONFIG_HOME/synche` or `$HOME/.config/synche`
-   **macOS**: `$HOME/Library/Application Support/synche`
-   **Windows**: `%APPDATA%\synche`

You can edit this file to configure the `home_path` and directories or manage everything in the Web GUI. Changes take effect immediately without restarting.
