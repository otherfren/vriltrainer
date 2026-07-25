//! One process per domain (D24). Started twice with different `--locale` and `--listen`, against
//! the same database file.

use std::sync::Arc;

use clap::Parser;
use server::config::{Cli, Config};
use server::db::Db;
use server::http::{self, AppState};
use server::pool::Manifest;
use server::trial::token::Sealer;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config = Config::from_cli(Cli::parse());
    init_tracing();

    let db = Db::open(&config.db_path)?;

    // The startup chain walk D24 requires. The UNIQUE constraints catch a fork at the moment of
    // writing; this catches everything that reached the file some other way — a hand-edited row, a
    // partial restore, two backups merged — and refusing to start is the right answer, because the
    // alternative is appending to a record that is already wrong.
    let entries = db.verify_chain()?;

    let pool = load_pool(&config)?;
    tracing::info!(
        locale = config.locale.code(),
        domain = config.locale.domain(),
        listen = %config.listen,
        db = %config.db_path.display(),
        log_entries = entries,
        pool_version = pool.version,
        pool_images = pool.images.len(),
        "vriltrainer starting"
    );

    let state = AppState {
        db: Arc::new(db),
        sealer: Arc::new(Sealer::new(&token_key(&config)?)),
        pool: Arc::new(pool),
        config: Arc::new(config.clone()),
    };

    let listener = tokio::net::TcpListener::bind(config.listen).await?;
    axum::serve(listener, http::router(state))
        .with_graceful_shutdown(shutdown())
        .await?;
    Ok(())
}

fn init_tracing() {
    use tracing_subscriber::EnvFilter;
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()))
        .init();
}

/// Loads and validates the published manifest.
///
/// A manifest that does not validate is refused here rather than at the first trial. Every trial
/// records the hash it was drawn under, so serving from a manifest whose hash does not match its
/// contents publishes trials nobody can recompute — and unlike a crash, that failure is silent.
fn load_pool(config: &Config) -> Result<Manifest, Box<dyn std::error::Error>> {
    let raw = std::fs::read_to_string(&config.pool_path)
        .map_err(|e| format!("cannot read pool manifest {}: {e}", config.pool_path.display()))?;
    let manifest: Manifest = serde_json::from_str(&raw)?;
    manifest.validate().map_err(|e| format!("pool manifest is not usable: {e}"))?;
    Ok(manifest)
}

/// The key that seals trial tokens (D16).
///
/// From a file when one is configured, so a restart does not orphan every open trial. That is not
/// convenience: an orphaned trial is an abandoned trial in the public record, and the abandonment
/// rate is a published figure (FR-021, SC-012) — a deploy must not move it.
fn token_key(config: &Config) -> Result<[u8; 32], Box<dyn std::error::Error>> {
    let Some(path) = &config.token_key_path else {
        tracing::warn!(
            "no --token-key: sealing with a fresh key, so every open trial dies on restart \
             and counts as abandoned"
        );
        return Ok(Sealer::random_key());
    };
    let text = std::fs::read_to_string(path)
        .map_err(|e| format!("cannot read token key {}: {e}", path.display()))?;
    let bytes = hex::decode(text.trim())?;
    bytes
        .try_into()
        .map_err(|_| format!("token key {} must be 64 hex characters", path.display()).into())
}

/// systemd sends SIGTERM to stop a unit on deploy.
///
/// Without handling it the process dies mid-request. SQLite rolls back a torn transaction, so the
/// log itself survives — what does not is the response: a trial whose COMMIT entry was written and
/// whose reply never arrived is an abandoned trial in the public record. The abandonment rate is
/// published (FR-021, SC-012), so a deploy that truncates requests moves a number the site makes
/// claims about.
async fn shutdown() {
    use tokio::signal::unix::{SignalKind, signal};

    let mut term = signal(SignalKind::terminate()).expect("SIGTERM handler installs");
    let mut int = signal(SignalKind::interrupt()).expect("SIGINT handler installs");
    tokio::select! {
        _ = term.recv() => {}
        _ = int.recv() => {}
    }
    tracing::info!("shutting down, draining in-flight requests");
}
