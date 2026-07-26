//! `verify_log --db <PATH>` — walks the hash chain of a database file and reports its head.
//!
//! The backup job's other half. Taking a snapshot proves only that bytes were copied; this proves
//! the copy is still an audit log — gapless sequence, every `prev_hash` linking, every
//! `entry_hash` recomputing. A backup that has not been walked is a file, not a record, and the
//! difference only shows up on the day it is restored.
//!
//! It exists as a binary rather than as a flag on the service because of what the service does
//! with a broken chain: it refuses to start (D24). That is right for the live file and useless for
//! an archived one, where the whole point is to learn the answer without a domain going down.
//!
//! Run it against a *copy*. `Db::open` applies pending migrations, so pointing it at an archived
//! snapshot would rewrite the artefact being checked.
//!
//! Exit codes are the interface: 0 the chain verifies, 1 it does not, 2 the file could not be
//! read. A backup script branches on that and needs no output parsing.

use std::path::PathBuf;

use clap::Parser;
use server::db::Db;

#[derive(Debug, Parser)]
#[command(
    name = "verify-log",
    version,
    about = "Walk the audit log's hash chain and report its head (D24)"
)]
struct Cli {
    /// The database to walk. A restored snapshot, normally — not the live file.
    #[arg(long, value_name = "PATH", default_value = "vriltrainer.db")]
    db: PathBuf,

    /// Fail unless the chain is at least this long.
    ///
    /// The chain walk alone cannot catch truncation: the first N entries of a valid log are
    /// themselves a valid log. Only comparing against what the previous backup held does, so the
    /// caller passes that count in and a snapshot that lost the tail is refused here rather than
    /// discovered at restore time.
    #[arg(long, value_name = "N")]
    at_least: Option<u64>,
}

fn main() -> std::process::ExitCode {
    let cli = Cli::parse();

    let db = match Db::open(&cli.db) {
        Ok(db) => db,
        Err(e) => {
            eprintln!("verify_log: {}: {e}", cli.db.display());
            return std::process::ExitCode::from(2);
        }
    };

    let entries = match db.verify_chain() {
        Ok(n) => n,
        Err(e) => {
            eprintln!("verify_log: chain broken in {}: {e}", cli.db.display());
            return std::process::ExitCode::FAILURE;
        }
    };

    if let Some(floor) = cli.at_least
        && entries < floor
    {
        eprintln!(
            "verify_log: {} verifies but holds {entries} entries, fewer than the {floor} expected \
             — the log is append-only, so this snapshot is short of the tail",
            cli.db.display()
        );
        return std::process::ExitCode::FAILURE;
    }

    // stdout carries the count alone so a caller can read it straight into a variable; everything
    // said to a human goes to stderr.
    println!("{entries}");
    eprintln!("{}: {entries} entries, chain verifies", cli.db.display());
    std::process::ExitCode::SUCCESS
}
