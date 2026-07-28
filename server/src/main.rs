//! One process per domain (D24). Started twice with different `--locale` and `--listen`, against
//! the same database file.

use std::path::Path;
use std::sync::Arc;

use clap::Parser;
use server::config::{Cli, Command, Config};
use server::db::Db;
use server::http::{self, AppState};
use server::pool::{Manifest, embedded};
use server::trial::token::Sealer;
use time::OffsetDateTime;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();
    init_tracing();

    if let Some(Command::Metrics { since }) = &cli.command {
        return print_metrics(&cli.db, since.as_deref());
    }

    let config = Config::from_cli(cli);
    let db = Db::open(&config.db_path)?;

    // The startup chain walk D24 requires. The UNIQUE constraints catch a fork at the moment of
    // writing; this catches everything that reached the file some other way — a hand-edited row, a
    // partial restore, two backups merged — and refusing to start is the right answer, because the
    // alternative is appending to a record that is already wrong.
    let entries = db.verify_chain()?;

    let pool = load_pool(&config)?;
    check_images_shipped(&pool)?;
    record_pool(&db, &pool)?;
    tracing::info!(
        locale = config.locale.code(),
        domain = config.locale.domain(),
        listen = %config.listen,
        db = %config.db_path.display(),
        log_entries = entries,
        pool_version = pool.version,
        pool_images = pool.images.len(),
        pool_bytes_embedded = embedded::count(),
        "vriltrainer starting"
    );

    let state = AppState {
        db: Arc::new(db),
        sealer: Arc::new(Sealer::new(&token_key(&config)?)),
        pool: Arc::new(pool),
        config: Arc::new(config.clone()),
        metrics: Arc::new(server::metrics::Metrics::new(
            config.locale,
            &server::db::now_rfc3339(),
        )),
    };

    // D23's fifteen-minute pass. Started before the listener, so the first visitor after a deploy
    // reads ranks this process computed rather than paying for the pass themselves.
    server::tasks::spawn_rank_timer(Arc::clone(&state.db), Arc::clone(&state.config));
    server::tasks::spawn_metrics_flush(Arc::clone(&state.db), Arc::clone(&state.metrics));

    let listener = tokio::net::TcpListener::bind(config.listen).await?;
    // `service`, not `router`: without `ConnectInfo` no handler learns the peer, so the forwarded
    // client address is unusable and every request counts against one bucket — the per-address
    // creation limit would throttle all users together (R8).
    axum::serve(listener, http::service(state))
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
    let raw = std::fs::read_to_string(&config.pool_path).map_err(|e| {
        format!(
            "cannot read pool manifest {}: {e}",
            config.pool_path.display()
        )
    })?;
    let manifest: Manifest = serde_json::from_str(&raw)?;
    manifest
        .validate()
        .map_err(|e| format!("pool manifest is not usable: {e}"))?;
    Ok(manifest)
}

/// Writes the served pool into `pool_version` and `pool_image`.
///
/// The manifests themselves are files on disk, and a lost `v1.json` is every trial recorded under
/// v1 made unrecomputable. This puts the version, its hash and its image list into the database —
/// which is the artefact that is backed up hourly, verified, and exported as a document that
/// carries every row of every table.
///
/// A version that now points at a different manifest is a warning and not a refusal: D34 makes the
/// version a pointer and binds each trial to a hash instead, so re-cutting is allowed. It must not
/// be silent, though — every trial committed under the old hash is verifiable only against a
/// manifest this service has stopped serving.
fn record_pool(db: &Db, pool: &Manifest) -> Result<(), Box<dyn std::error::Error>> {
    use server::pool::record::{self, Recorded};

    match record::served(db, pool, &server::db::now_rfc3339())? {
        Recorded::First => tracing::info!(
            version = pool.version,
            manifest_hash = %pool.manifest_hash,
            images = pool.images.len(),
            "pool version recorded"
        ),
        Recorded::Unchanged => {}
        Recorded::Recut { was } => tracing::warn!(
            version = pool.version,
            was = %was,
            now = %pool.manifest_hash,
            "pool version re-cut: trials committed under the previous hash are verifiable only \
             against a manifest this service no longer serves. Keep the old v{}.json.",
            pool.version
        ),
    }
    Ok(())
}

/// `vriltrainer metrics --since` (T114, D28).
///
/// Tab-separated, oldest first, one line per day, locale and metric. Deliberately not JSON and
/// deliberately not a route: these are operator figures, the public admin API is name approval and
/// nothing else (D25), and a table that `sort`, `cut` and `awk` read is the one an operator on the
/// box already has tools for.
///
/// `trials_abandoned` is not a counter and is not printed. Abandonment is the *absence* of a
/// resolve entry after D16's clock runs out, so there is no moment at which anything could
/// increment it — the published figure comes from the log, where a reader can recount it, and
/// `GET /api/stats/aggregate` is where it is reported.
fn print_metrics(db_path: &Path, since: Option<&str>) -> Result<(), Box<dyn std::error::Error>> {
    let db = Db::open(db_path)?;
    let since = match since {
        Some(day) => day.to_string(),
        None => default_since(),
    };

    println!("day\tlocale\tmetric\tcount");
    for row in server::metrics::since(&db, &since)? {
        println!("{}\t{}\t{}\t{}", row.day, row.locale, row.metric, row.count);
    }
    Ok(())
}

/// Thirty days back, which is a month of traffic and shorter than the account grace period — long
/// enough to see a trend, short enough that the default is not a wall of text.
fn default_since() -> String {
    let now = OffsetDateTime::now_utc() - time::Duration::days(30);
    format!(
        "{:04}-{:02}-{:02}",
        now.year(),
        now.month() as u8,
        now.day()
    )
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

/// Refuses to start if the manifest names an image this build does not carry (D29).
///
/// The images are compiled in, so ordinarily this cannot fail — the same `poolctl import` that
/// produced `pool/normalised/` produced the catalogue the manifest was cut from. It can fail in
/// exactly one way, and it is the way that matters: `--pool` pointing at a manifest from a
/// different build. Version 2's manifest against version 1's binary starts cleanly, draws trials
/// normally, and serves eight broken pictures — a failure that looks like a client bug, produces
/// no log line, and is discovered by a visitor.
///
/// Reported by count and by example rather than in full: a mismatched pool is every image, and a
/// hundred and eighty-nine identifiers scrolling past says nothing the first three do not.
fn check_images_shipped(pool: &Manifest) -> Result<(), Box<dyn std::error::Error>> {
    let missing = embedded::missing(pool.images.iter().map(|i| i.id.as_str()));
    if missing.is_empty() {
        return Ok(());
    }
    let examples: Vec<&str> = missing.iter().take(3).copied().collect();
    Err(format!(
        "this build does not carry {} of the {} images in pool v{} (e.g. {}). \
         The pool is compiled in: rebuild after `poolctl import`, or point --pool at the manifest \
         this binary was built from.",
        missing.len(),
        pool.images.len(),
        pool.version,
        examples.join(", ")
    )
    .into())
}
