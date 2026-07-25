//! `admin_key --db <PATH> --rotate` — issues the key to the public admin API and prints it once.
//!
//! A separate binary rather than a subcommand of the service, and that is a compromise worth
//! naming. D25 writes it as `server admin-key --rotate`; the service's `--locale` deliberately has
//! no default (D24), so folding this in would mean either demanding a locale a rotation has no use
//! for, or giving that flag a default and losing the property D24 exists to have — a mistyped unit
//! file that starts a second German instance on the English domain. The key is not a per-domain
//! thing: both processes read one database, so one rotation serves both.
//!
//! It stays a command behind SSH rather than an endpoint because of what it *is*. The public admin
//! API performs only reversible operations, and everything that changes what that API is stays
//! here — an endpoint that reissues the key would be the destructive operation D25 refuses to put
//! on a public surface.
//!
//! The key goes to stdout alone and everything else to stderr, so `admin_key --rotate > key` and a
//! pipe into a password manager both do the obvious thing.

use std::path::PathBuf;

use clap::Parser;
use server::account::admin_key;
use server::db::{Db, now_rfc3339};

#[derive(Debug, Parser)]
#[command(
    name = "admin-key",
    version,
    about = "Rotate the key to the vriltrainer admin API (D25)"
)]
struct Cli {
    /// The service's database. The hash lives there rather than in an environment file precisely
    /// so that this takes effect on the next request, with nothing restarted.
    #[arg(long, value_name = "PATH", default_value = "vriltrainer.db")]
    db: PathBuf,

    // Required rather than implied by running the tool at all: with no argument doing the thing,
    // an idle `admin_key` typed to see what it does would invalidate the reviewers' key.
    /// Issue a new key and retire the one in use.
    #[arg(long)]
    rotate: bool,

    /// Who this key is for. It appears in no response; it is there so an operator can tell one
    /// rotation from the next in the table.
    #[arg(long, value_name = "TEXT", default_value = "operator")]
    label: String,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();
    if !cli.rotate {
        eprintln!("nothing to do — pass --rotate to issue a new key");
        std::process::exit(2);
    }

    // Opens and migrates, exactly as the service does. The chain is deliberately *not* verified:
    // this touches no log entry, and a rotation must stay possible on a day the log is the thing
    // being investigated.
    let db = Db::open(&cli.db)?;
    let now = now_rfc3339();
    let rotation = admin_key::rotate(&db, &cli.label, &now)?;

    println!("{}", rotation.key);
    eprintln!(
        "key {} issued as '{}' at {} — {} previous key(s) retired, no restart needed.\n\
         This is the only time it is printed; the database holds a hash.",
        rotation.id, cli.label, now, rotation.revoked
    );
    Ok(())
}
