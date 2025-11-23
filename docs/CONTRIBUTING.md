# Contributing & Feedback

**Synche** is in active development, and contributions are welcome. There are multiple **[issues](https://github.com/matx64/synche/issues)** opened but if you find a bug, have a feature request, or want to contribute, please open an issue or submit a pull request.

## Roadmap

The roadmap below outlines the high-level goals and major milestones for the project. Please check the [Issues](https://github.com/matx64/synche/issues) to see if work has already started, or open a new discussion to coordinate with the maintainers. We also welcome smaller improvements, bug fixes, and documentation updates not explicitly listed here.

-   [x] Local network device discovery (mDNS)
-   [x] File watcher and P2P sync over TCP
-   [x] Version vectors for conflict resolution and integrity checks
-   [x] SQLite persistence for metadata
-   [x] `.gitignore` support
-   [x] Web GUI
-   [ ] General Alpha improvements
-   [ ] Transfer file blocks instead of the whole file
-   [ ] Performance and resource optimization

## Development Workflow

Synche is primarily written in Rust with a simple web frontend.

### Prerequisites

- [Rust and Cargo](https://rustup.rs/) (latest stable version)
- [Git](https://git-scm.com/downloads)
- [Just](https://github.com/casey/just) and [Watchexec](https://github.com/watchexec/watchexec) (optional, but useful for running project commands)

Check the [Build Guide](BUILD.md). Optionally, you can use `watchexec + just` tools for iterating during development, by executing `just dev` in your terminal.

### Making Changes

1.  Create a new branch for your feature or fix:
    ```bash
    git checkout -b feature/my-new-feature
    ```
2.  Implement your changes.
3.  Ensure your code is formatted according to the project standards:
    ```bash
    cargo fmt
    ```
4.  Run clippy to catch common mistakes:
    ```bash
    cargo clippy
    ```

## Pull Request Process

1.  Update the documentation if you are changing functionality.
2.  Add tests for any new features or bug fixes.
3.  Ensure all tests pass and there are no linting errors.
4.  Push your branch to your fork and submit a Pull Request to the `main` branch of the `matx64/synche` repository.
5.  Provide a clear description of the changes and link to any relevant issues.
