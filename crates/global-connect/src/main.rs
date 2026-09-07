//! Global Connect: the public site.
//!
//! A third binary beside `phonix-server` and `phonix-desk`, reading the same
//! configuration file and serving the one surface that is in front of the
//! sign-in box rather than behind it.
//!
//! Read `docs/adr/0007-global-connect.md` before changing any of it. The three
//! decisions that shape everything below:
//!
//! * **It depends on nothing that can be down.** No database, no cache, no
//!   broker, no mail server. The day Postgres is down this is still serving,
//!   which is also the day traffic arrives at it.
//! * **It collects nothing.** There is no `POST` route. Every call to action is
//!   a link to `/signup`, where the product already knows how to create a
//!   workspace.
//! * **Loopback, with nginx in front** on `www.<base_domain>` and the apex. A
//!   public bind answers any `Host` header and can be reached by address, which
//!   on a box that also serves workspaces means the marketing site answering
//!   for a tenant.
//!
//! # Usage
//!
//! ```text
//! global-connect      serve
//! ```

mod artifacts;
mod assets;
mod i18n;
mod limit;
mod routes;
mod state;

use std::process::ExitCode;
use std::sync::Arc;

use anyhow::{Context, Result};
use phonix_config::{AppConfig, ConfigError};

use crate::state::SiteState;

fn main() -> ExitCode {
    // Configuration and telemetry come up before the runtime, so a misconfigured
    // process fails with a plain message rather than a panic inside a worker.
    let config = match phonix_config::load() {
        Ok(config) => config,
        Err(err) => {
            report_config_error(&err);
            return ExitCode::FAILURE;
        }
    };

    let _telemetry =
        match phonix_telemetry::init(&config.telemetry, &config.app.environment, Vec::new()) {
            Ok(guard) => guard,
            Err(err) => {
                eprintln!("failed to initialise logging: {err}");
                return ExitCode::FAILURE;
            }
        };

    let runtime = match tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
    {
        Ok(runtime) => runtime,
        Err(err) => {
            tracing::error!(error = %err, "failed to build the tokio runtime");
            return ExitCode::FAILURE;
        }
    };

    match Command::from_args(std::env::args().skip(1)) {
        Ok(Command::Serve) => match runtime.block_on(serve(config)) {
            Ok(()) => ExitCode::SUCCESS,
            Err(err) => {
                tracing::error!(error = format!("{err:#}"), "global-connect failed");
                eprintln!("global-connect failed: {err:#}");
                ExitCode::FAILURE
            }
        },
        Err(usage) => {
            eprintln!("{usage}");
            ExitCode::FAILURE
        }
    }
}

/// What the process was asked to do.
///
/// One command, and it is the default. The type exists so that
/// `global-connect --help` is refused rather than silently starting a server -
/// the same failure `phonix-desk` guards against.
enum Command {
    Serve,
}

const USAGE: &str = "usage:\n  global-connect      serve the public site";

impl Command {
    fn from_args(mut args: impl Iterator<Item = String>) -> Result<Self, &'static str> {
        match args.next().as_deref() {
            None | Some("serve") => Ok(Self::Serve),
            Some(_) => Err(USAGE),
        }
    }
}

async fn serve(config: AppConfig) -> Result<()> {
    let listen = config.site.listen.clone();
    let state = SiteState::new(Arc::new(config));

    let listener = tokio::net::TcpListener::bind(&listen)
        .await
        .with_context(|| format!("could not bind {listen}"))?;

    tracing::info!(
        address = %listen,
        environment = %state.environment(),
        signup = %state.links().signup,
        "global-connect is listening"
    );

    // `into_make_service_with_connect_info` rather than `into_make_service`,
    // because the rate limiter falls back to the peer address when no proxy
    // header is configured - and without this the extension is simply absent
    // and every visitor shares one key.
    let app = routes::router(state)
        .into_make_service_with_connect_info::<std::net::SocketAddr>();

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await
        .context("the site server stopped unexpectedly")?;

    tracing::info!("global-connect stopped cleanly");
    Ok(())
}

/// Stop on Ctrl-C or SIGTERM, so systemd's `stop` is not a kill.
async fn shutdown_signal() {
    let interrupt = async {
        if let Err(err) = tokio::signal::ctrl_c().await {
            tracing::error!(error = %err, "could not listen for ctrl-c");
            std::future::pending::<()>().await;
        }
    };

    #[cfg(unix)]
    let terminate = async {
        match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
            Ok(mut signal) => {
                signal.recv().await;
            }
            Err(err) => tracing::error!(error = %err, "could not listen for SIGTERM"),
        }
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        () = interrupt => tracing::info!("interrupt received, shutting down"),
        () = terminate => tracing::info!("SIGTERM received, shutting down"),
    }
}

fn report_config_error(err: &ConfigError) {
    eprintln!("global-connect could not start: {err}");

    if let ConfigError::MissingBase(path) = err {
        eprintln!();
        eprintln!("Expected to find configuration at: {}", path.display());
        eprintln!("Run from the workspace root, or set CARGO_MANIFEST_DIR.");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_arguments_means_serve() {
        assert!(matches!(
            Command::from_args(std::iter::empty()),
            Ok(Command::Serve)
        ));
    }

    /// `global-connect --help` must not start a listener.
    #[test]
    fn an_unknown_command_is_refused_rather_than_served() {
        assert!(Command::from_args(["--help".to_owned()].into_iter()).is_err());
    }
}
