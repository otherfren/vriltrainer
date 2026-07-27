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

/// One rung of the ladder, and its mirror image below Normie.
///
/// The two slugs share one edge because the symmetry is the argument: under chance the tails are
/// equally populated, so if the site keeps producing about as many Kartoffeln as Annunaki, that
/// ratio *is* the significance test, readable by anyone without statistics. Storing the pair
/// together means the ladder cannot drift out of symmetry through an edit to one end.
///
/// The edge is a distance from chance, not a share of the population (D31, superseding D23). A
/// share made a rank a statement about who else showed up: a player could be demoted by strangers
/// signing up, and the tail-versus-tail comparison was true by construction and therefore proved
/// nothing. A sigma edge is computed from one account's own trials and hits, which means a holder
/// can check their own rank without trusting the server's sort — the standard the rest of the site
/// is built to.
///
/// Slugs, not titles. The titles are product copy and live in the client's message catalogue,
/// which is what keeps a German string out of a Rust file (see CLAUDE.md).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RankBand {
    /// The upper band, e.g. `annunaki`.
    pub high: String,
    /// Its mirror below Normie, e.g. `kartoffel`.
    pub low: String,
    /// Where the band starts, as an absolute deviation in standard deviations. An account is in
    /// the band when `|deviation| >= from_sigma` and below the next rung's edge.
    pub from_sigma: f64,
}

impl RankBand {
    fn new(high: &str, low: &str, from_sigma: f64) -> Self {
        RankBand {
            high: high.into(),
            low: low.into(),
            from_sigma,
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
    ///
    /// Lowered from 100 to 30 because it was doing a job it was never needed for. The board sorts
    /// on the Wilson lower bound, and that bound already keeps short records off the top: a lucky
    /// 5-of-8 is assured of less than chance, so it cannot outrank a long steady record however
    /// good its rate looks. What the floor actually buys is a brake on account farming, and the
    /// expensive half of that brake is the calendar below, not the count.
    pub eligibility_trials: u32,
    /// Distinct UTC days those trials must span (D21, FR-040). Parallelism does not compress the
    /// calendar, which is the only reason this resists farming at all.
    pub eligibility_days: u32,
    /// The ladder, best band first — that is, by descending `from_sigma`. A band is awarded on an
    /// account's own deviation and exists at every population (D31).
    pub bands: Vec<RankBand>,
}

impl Default for Thresholds {
    fn default() -> Self {
        Thresholds {
            stats_unlock_at: 10,
            eligibility_trials: 30,
            eligibility_days: 2,
            // Even 0,8 σ steps out from a Normie band of ±0,3. The even step is not cosmetic: it
            // makes the eleven rungs eleven equal columns, so the distribution chart and the
            // ladder are one axis and a reader can lay them side by side.
            bands: vec![
                RankBand::new("annunaki", "kartoffel", 3.5),
                RankBand::new("loosh", "nullleiter", 2.7),
                RankBand::new("reptilian", "orgonit", 1.9),
                RankBand::new("grey", "erdstrahlen", 1.1),
                RankBand::new("asset", "pineal", 0.3),
            ],
        }
    }
}

impl Thresholds {
    /// The band edges as absolute distances from chance, nearest first.
    ///
    /// The reverse of [`Thresholds::bands`], which is best first. Both orders are used — the award
    /// walks one and the binning the other — so reading the edges from the same list is what keeps
    /// a ladder and a chart from disagreeing.
    pub fn edges(&self) -> Vec<f64> {
        self.bands.iter().rev().map(|b| b.from_sigma).collect()
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
    ///
    /// Ten rather than the twenty-five D17 named, and the same ten the statistics unlock at, so
    /// every boundary is a multiple of the unlock. Twenty-five meant a player watched the same
    /// rank for fifteen sessions after it first appeared, which reads as a broken page rather than
    /// as a held figure — and a held figure nobody believes buys no honesty. The protection is
    /// weaker at ten and still present: the number moves in steps a player cannot stop inside, and
    /// a rank still needs 100 trials over three days.
    pub block_size: u32,
    /// Seconds that must pass between reveal and answer (D21, FR-039).
    pub min_view_seconds: i64,
    /// How long a revealed trial may still be answered (D16).
    pub trial_lifetime_hours: i64,
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
    /// How long an account that appears nowhere in the log is kept before it is swept (D32).
    ///
    /// Thirty days, and deliberately generous. The token *is* the account and there is no recovery
    /// (D9), so being early destroys the bookmark of somebody who signed up, read the instructions
    /// and meant to come back — and that visitor has no way to tell anyone it happened. Being late
    /// costs one row holding a name, a public identifier and a token hash. Those two costs are not
    /// comparable, so the number is set where the wrong one cannot happen.
    ///
    /// A trial dies after `trial_lifetime_hours`, so anybody who ever started one is in the log
    /// long before this. The width is also what makes the sweep safe against a request in flight:
    /// no handler holds an account for thirty days between authenticating it and writing its commit.
    pub unused_account_grace_hours: i64,
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
            block_size: 10,
            min_view_seconds: 3,
            trial_lifetime_hours: 24,
            rename_cooldown_hours: 24,
            unused_account_grace_hours: 24 * 30,
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
    about = "The vriltrainer service - one process per domain"
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

    /// The ladder is a sequence of distances from chance, listed best first, read back nearest
    /// first. Both orders are load-bearing — the award walks one, the binning the other — so a
    /// band list sorted the wrong way has to fail here rather than silently invert the ladder.
    #[test]
    fn the_bands_are_ordered_by_distance_from_chance() {
        let t = Thresholds::default();
        assert_eq!(t.edges(), vec![0.3, 1.1, 1.9, 2.7, 3.5]);
        for pair in t.bands.windows(2) {
            assert!(
                pair[0].from_sigma > pair[1].from_sigma,
                "bands must be listed best first"
            );
        }
        assert!(
            t.bands.last().expect("a ladder").from_sigma > 0.0,
            "Normie has width"
        );
    }

    /// The steps are even, which is what lets eleven rungs be eleven equal chart columns. A test
    /// rather than a comment because the property is invisible in the literal list, and an edit
    /// that breaks it breaks the chart quietly.
    #[test]
    fn the_rungs_are_evenly_spaced() {
        let edges = Thresholds::default().edges();
        let step = edges[1] - edges[0];
        for pair in edges.windows(2) {
            assert!(
                (pair[1] - pair[0] - step).abs() < 1e-9,
                "{pair:?} is not one step of {step}"
            );
        }
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
