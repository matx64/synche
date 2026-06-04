use crate::domain::PortOverrides;
use clap::Parser;
use std::path::PathBuf;

/// Command-line arguments for the `synche` binary.
///
/// `--version` and `--help` are provided automatically by clap; `--version`
/// prints the crate version (`CARGO_PKG_VERSION`), the single source of truth.
///
/// Port flags override the optional `[ports]` block in `config.toml`, which in
/// turn overrides the built-in defaults (precedence CLI > config > default).
#[derive(Parser, Debug)]
#[command(
    name = "synche",
    version,
    about = "Local-network peer-to-peer file sync",
    long_about = None,
)]
pub struct Cli {
    /// Root directory for all Synche state (config, data, logs); overrides the
    /// OS-default directories. Lets two instances run side-by-side on one host.
    #[arg(long, value_name = "PATH")]
    pub config_dir: Option<PathBuf>,

    /// Port for the web GUI / HTTP API (default 42880).
    #[arg(long, value_name = "PORT")]
    pub http_port: Option<u16>,

    /// Port advertised for mDNS presence/discovery (default 42881).
    #[arg(long, value_name = "PORT")]
    pub presence_port: Option<u16>,

    /// TCP port for peer transport — handshakes, metadata, transfers (default 42882).
    #[arg(long, value_name = "PORT")]
    pub transport_port: Option<u16>,
}

impl Cli {
    /// Collects the three port flags into a [`PortOverrides`]. Unset flags stay
    /// `None` so they fall back to config and then defaults.
    pub fn port_overrides(&self) -> PortOverrides {
        PortOverrides {
            http: self.http_port,
            presence: self.presence_port,
            transport: self.transport_port,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;

    #[test]
    fn cli_definition_is_valid() {
        Cli::command().debug_assert();
    }

    #[test]
    fn parses_port_flags() {
        let cli = Cli::try_parse_from([
            "synche",
            "--http-port",
            "9999",
            "--config-dir",
            "/tmp/synche-a",
        ])
        .unwrap();

        assert_eq!(cli.http_port, Some(9999));
        assert_eq!(
            cli.config_dir.as_deref(),
            Some(std::path::Path::new("/tmp/synche-a"))
        );

        let overrides = cli.port_overrides();
        assert_eq!(overrides.http, Some(9999));
        assert_eq!(overrides.presence, None);
        assert_eq!(overrides.transport, None);
    }

    #[test]
    fn bare_args_leave_all_overrides_unset() {
        let cli = Cli::try_parse_from(["synche"]).unwrap();
        assert!(cli.port_overrides().is_empty());
        assert!(cli.config_dir.is_none());
    }
}
