//! Startup configuration.
//!
//! The thresholds here are configuration rather than constants because D26 requires it: they are
//! expected to move — the bars start low while the site is unknown and rise as activity justifies
//! it — and every response that depends on one reports it, so nobody loses a rank to a number they
//! could not see. A constant compiled into the client would break both halves of that.

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};
use std::path::PathBuf;

use clap::{Parser, ValueEnum};
use serde::Serialize;

/// Which frontend this process serves.
///
/// D24 makes this a hard startup switch and **not** the `Host` header. Two instances of the same
/// binary run, one per domain, and a process started as the German one cannot serve English by
/// accident — so a proxy misconfiguration fails visibly instead of quietly serving the wrong
/// language, which is what the `Host`-header design would have done.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Locale {
    De,
    En,
}

impl Locale {
    pub fn code(self) -> &'static str {
        match self {
            Locale::De => "de",
            Locale::En => "en",
        }
    }

    /// The domain this locale is served from (D10, one language per domain).
    pub fn domain(self) -> &'static str {
        match self {
            Locale::De => "vriltrainer.de",
            Locale::En => "vriltrainer.com",
        }
    }
}

/// One rung of the D23 ladder, and its mirror image below Normie.
///
/// The two slugs share a share because the symmetry is the argument: under chance the tails are
/// equally populated, so if the site keeps producing about as many Kartoffeln as Annunaki, that
/// ratio *is* the significance test, readable by anyone without statistics. Storing the pair
/// together means the ladder cannot drift out of symmetry through an edit to one end.
///
/// Slugs, not titles. The titles are product copy and live in the client's message catalogue,
/// which is what keeps a German string out of a Rust file (see CLAUDE.md).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RankBand {
    /// The upper band, e.g. `annunaki`.
    pub high: String,
    /// Its mirror below Normie, e.g. `kartoffel`.
    pub low: String,
    /// The share of the eligible population each of the two holds.
    pub share: f64,
}

impl RankBand {
    fn new(high: &str, low: &str, share: f64) -> Self {
        RankBand {
            high: high.into(),
            low: low.into(),
            share,
        }
    }
}

/// The numbers D26 names, in the form the API reports them.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Thresholds {
    /// Completed trials before the statistics view appears (D8). Gated on count, never on
    /// success: conditioning on a hit would make the page overstate the very rate it exists to
    /// report honestly.
    pub stats_unlock_at: u32,
    /// Completed trials before an account is leaderboard-eligible (D17, FR-040).
    pub eligibility_trials: u32,
    /// Distinct UTC days those trials must span (D21, FR-040). Parallelism does not compress the
    /// calendar, which is the only reason this resists farming at all.
    pub eligibility_days: u32,
    /// The ladder, best band first. A band is awarded once `share * eligible >= 1`, with no
    /// rounding up — rounding would hand out the rarest title at any population, which is the
    /// opposite of what a share means (D23).
    pub bands: Vec<RankBand>,
}

impl Default for Thresholds {
    fn default() -> Self {
        Thresholds {
            stats_unlock_at: 10,
            eligibility_trials: 100,
            eligibility_days: 3,
            bands: vec![
                RankBand::new("annunaki", "kartoffel", 0.001),
                RankBand::new("loosh", "nullleiter", 0.005),
                RankBand::new("reptilian", "orgonit", 0.02),
                RankBand::new("grey", "erdstrahlen", 0.07),
                RankBand::new("asset", "pineal", 0.20),
            ],
        }
    }
}

impl Thresholds {
    /// The smallest eligible population at which `band` exists at all.
    pub fn band_unlocks_at(&self, band: &RankBand) -> u64 {
        (1.0 / band.share).ceil() as u64
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Config {
    pub locale: Locale,
    pub db_path: PathBuf,
    /// The published pool manifest. Refused at startup if it does not validate.
    pub pool_path: PathBuf,
    /// Where to bind. Localhost by default: nginx terminates TLS and proxies (R8).
    pub listen: SocketAddr,
    /// The built client bundle to serve. nginx proxies every path here, so with this unset the
    /// site answers `/api` and nothing else — no page, no scripts, no images.
    pub public_dir: Option<PathBuf>,
    /// Peers whose forwarded client-address header is believed. Anything else is taken at its
    /// socket address, or any client could forge its own and the per-address limit is gone (R8).
    pub trusted_proxies: Vec<IpAddr>,
    /// Optional file holding the 32-byte token key as hex. Without it the key is fresh per
    /// process, and every open trial dies on restart — which inflates the *published* abandonment
    /// rate (FR-021, SC-012), so this is a correctness setting and not a convenience.
    pub token_key_path: Option<PathBuf>,
    pub thresholds: Thresholds,
    /// Completed trials per reported block (D17). Block-wise reporting is what blunts optional
    /// stopping: a user who plays until the number looks good otherwise inflates the
    /// false-positive rate far past 5 %.
    pub block_size: u32,
    /// Seconds that must pass between reveal and answer (D21, FR-039).
    pub min_view_seconds: i64,
    /// How long a revealed trial may still be answered (D16).
    pub trial_lifetime_hours: i64,
    /// Concurrent uncompleted trials per account. Every trial is permanent, so this — not a rate
    /// limit over time — is what bounds the growth of the log (D17).
    pub open_trials_per_account: u32,
    /// Accounts one client address may create per hour (D17).
    pub accounts_per_address_per_hour: u32,
    /// Hours a holder must wait between name submissions (FR-048, D25).
    ///
    /// The scarce resource being rationed is the reviewer, not the database, so this is a cooldown
    /// per account rather than a quota: one name in the queue per account per day is a queue a
    /// person can still clear. A rejection clears the clock, because the user has not had a turn.
    ///
    /// It runs from **submission** and not from the decision, which is what holds a name still
    /// while it is being looked at. Were a pending name editable, a user could swap it between the
    /// moment the reviewer read the queue and the moment they clicked approve, and a human would
    /// have published a name nobody ever saw — the outcome pre-approval exists to prevent.
    pub rename_cooldown_hours: i64,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            // The `.de` domain is the primary one; a default is needed for tests, never for a
            // running process — the CLI flag has none, see `Cli::locale`.
            locale: Locale::De,
            db_path: PathBuf::from("vriltrainer.db"),
            pool_path: PathBuf::from("pool/manifest.json"),
            listen: SocketAddr::from((Ipv4Addr::LOCALHOST, 8080)),
            public_dir: None,
            trusted_proxies: vec![
                IpAddr::V4(Ipv4Addr::LOCALHOST),
                IpAddr::V6(Ipv6Addr::LOCALHOST),
            ],
            token_key_path: None,
            thresholds: Thresholds::default(),
            block_size: 25,
            min_view_seconds: 3,
            trial_lifetime_hours: 24,
            open_trials_per_account: 3,
            accounts_per_address_per_hour: 5,
            rename_cooldown_hours: 24,
        }
    }
}

impl Config {
    pub fn from_cli(cli: Cli) -> Self {
        let defaults = Config::default();
        Config {
            locale: cli.locale,
            db_path: cli.db,
            pool_path: cli.pool,
            listen: cli.listen,
            public_dir: cli.public,
            trusted_proxies: if cli.trusted_proxy.is_empty() {
                defaults.trusted_proxies
            } else {
                cli.trusted_proxy
            },
            token_key_path: cli.token_key,
            ..defaults
        }
    }
}

#[derive(Debug, Parser)]
#[command(
    name = "vriltrainer",
    version,
    about = "The vriltrainer service — one process per domain"
)]
pub struct Cli {
    #[arg(long, value_name = "PATH", default_value = "vriltrainer.db")]
    pub db: PathBuf,

    #[arg(long, value_name = "PATH", default_value = "pool/manifest.json")]
    pub pool: PathBuf,

    #[arg(long, value_name = "ADDR", default_value = "127.0.0.1:8080")]
    pub listen: SocketAddr,

    /// Directory holding the built client bundle, served with an SPA fallback.
    #[arg(long, value_name = "DIR")]
    pub public: Option<PathBuf>,

    // Deliberately has no default. D24 exists so the locale is chosen once, visibly, in the unit
    // file; a default would let a mistyped flag start a second German instance on the English
    // domain and nothing would look wrong.
    /// The language this process serves.
    #[arg(long, value_enum)]
    pub locale: Locale,

    /// Repeatable. Peers whose forwarded client address is believed (R8).
    #[arg(long = "trusted-proxy", value_name = "IP")]
    pub trusted_proxy: Vec<IpAddr>,

    /// File holding the trial token key as 64 hex characters.
    #[arg(long = "token-key", value_name = "PATH")]
    pub token_key: Option<PathBuf>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;

    #[test]
    fn the_cli_is_well_formed() {
        Cli::command().debug_assert();
    }

    /// D24's whole point. If this ever gains a default, a misconfigured unit serves the wrong
    /// language silently instead of refusing to start.
    #[test]
    fn locale_must_be_given_explicitly() {
        assert!(Cli::try_parse_from(["vriltrainer"]).is_err());
        let cli = Cli::try_parse_from(["vriltrainer", "--locale", "en"]).unwrap();
        assert_eq!(Config::from_cli(cli).locale, Locale::En);
    }

    /// D23's rule, read off the configuration rather than restated in code.
    #[test]
    fn bands_unlock_as_the_population_grows() {
        let t = Thresholds::default();
        let at: Vec<u64> = t.bands.iter().map(|b| t.band_unlocks_at(b)).collect();
        assert_eq!(at, vec![1000, 200, 50, 15, 5]);
    }

    /// The ladder is symmetric by construction, so no edit can leave one tail wider than the
    /// other — the ratio between the tails is the significance test D19 and D23 rest on.
    #[test]
    fn every_band_has_a_mirror() {
        let t = Thresholds::default();
        assert_eq!(
            t.bands.len(),
            5,
            "five bands each side of Normie makes eleven rungs"
        );
        for b in &t.bands {
            assert_ne!(b.high, b.low);
        }
    }
}
