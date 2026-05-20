//! Subscriber setup for the whole binary.
//!
//! Initialised exactly once from `main`. We don't expose anything for tests
//! to call — `tracing_subscriber::registry().init()` is a process-global
//! action and `cargo test` runs every test in the same process, so a test
//! that called `init` would race with all the others.
use std::{io::IsTerminal, path::Path};

use tracing_appender::{
    non_blocking::WorkerGuard,
    rolling::{RollingFileAppender, Rotation},
};
use tracing_subscriber::{EnvFilter, fmt, layer::SubscriberExt, util::SubscriberInitExt};

/// Daily log files kept before the appender prunes older ones on rotation.
const MAX_LOG_FILES: usize = 14;

/// Holds the background-writer guard for the file appender. Must outlive the
/// process: dropping it shuts the writer down and discards any in-flight
/// log lines. `main` keeps it in a `_log_guards` binding for that reason.
pub struct LogGuards {
    _file: WorkerGuard,
}

/// Default `RUST_LOG` directive used when the env var is unset or unparseable.
fn default_directive() -> &'static str {
    if cfg!(debug_assertions) {
        "synche=debug,warn"
    } else {
        "synche=info,warn"
    }
}

/// Initialises the global `tracing` subscriber for the binary.
///
/// Pipes events to two layers: stdout (ANSI when stdout is a TTY,
/// plain text otherwise, no target) and a daily-rotated file under
/// `log_dir` (no ANSI, target included). Honours `RUST_LOG` and falls
/// back to `synche=debug,warn` in debug builds / `synche=info,warn`
/// in release.
///
/// The returned `LogGuards` MUST be held for the lifetime of `main`
/// — dropping it discards in-flight log lines.
pub fn init(log_dir: &Path) -> LogGuards {
    let file_appender = RollingFileAppender::builder()
        .rotation(Rotation::DAILY)
        .filename_prefix("synche.log")
        .max_log_files(MAX_LOG_FILES)
        .build(log_dir)
        .expect("failed to build rolling log appender");
    let (file_writer, file_guard) = tracing_appender::non_blocking(file_appender);

    let env_filter =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(default_directive()));

    let stdout_layer = fmt::layer()
        .with_target(false)
        .with_ansi(std::io::stdout().is_terminal());

    let file_layer = fmt::layer()
        .with_writer(file_writer)
        .with_ansi(false)
        .with_target(true);

    tracing_subscriber::registry()
        .with(env_filter)
        .with(stdout_layer)
        .with(file_layer)
        .init();

    LogGuards { _file: file_guard }
}
