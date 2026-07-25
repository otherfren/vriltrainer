//! The pre-filter, ported from the client's `checkDisplayName` (D25, T097).
//!
//! Since D25 this is no longer the gate — a human approves every name that reaches the board — it
//! is what keeps the review queue short. The client runs the same rules so it can tell somebody
//! what is wrong while they type, but that copy is UX only: a rule checked only in the client is
//! not checked.
//!
//! Both implementations must agree, so `client/src/app/core/display-name.ts` and this file move
//! together or names start being accepted in one place and refused in the other. The tests below
//! are `display-name.spec.ts`, case for case, which is what makes that claim checkable.

pub const NAME_MIN: usize = 3;
pub const NAME_MAX: usize = 20;

/// Why a name was refused.
///
/// A code rather than a sentence. The sentence is product copy, it differs per domain (D10), and
/// it belongs in the client's message catalogue — putting German in a Rust file would be the one
/// thing CLAUDE.md forbids outright.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Refusal {
    TooShort,
    TooLong,
    /// No letters at all: `1488`, punctuation, whitespace.
    Shapeless,
    /// A name the interface uses for itself.
    Reserved,
    Hate,
    Vulgar,
    /// An address of some kind — a URL, an email, a handle pretending to be one.
    Address,
}

/// Names the interface uses for itself, which nobody else gets to wear.
const RESERVED: [&str; 17] = [
    "admin",
    "administrator",
    "mod",
    "moderator",
    "system",
    "root",
    "server",
    "support",
    "team",
    "staff",
    "vriltrainer",
    "vril",
    "anonym",
    "anonymous",
    "null",
    "undefined",
    "nan",
];

/// Hate and extremist terms. This is a public leaderboard on a site whose subject matter is
/// adjacent to the Vril and Reichsflugscheibe mythology, so this list is not decoration — the
/// board would attract exactly this without it.
///
/// Matched as substrings of the folded name, so `SiegHeil88` and `h1tl3r` land on the same entry
/// as the plain spelling.
const HATE: [&str; 18] = [
    "hitler",
    "siegheil",
    "heilhitler",
    "nsdap",
    "waffenss",
    "schutzstaffel",
    "hakenkreuz",
    "swastika",
    "judensau",
    "untermensch",
    "rassenschande",
    "whitepower",
    "kukluxklan",
    "nigga",
    "nigger",
    "faggot",
    "kanacke",
    "zigeuner",
];

/// Ordinary vulgarity. Not a moral position, just not a name on a scoreboard.
const VULGAR: [&str; 11] = [
    "fotze",
    "wichser",
    "hurensohn",
    "arschloch",
    "schwanzlutscher",
    "bastard",
    "cunt",
    "fuck",
    "shit",
    "penis",
    "vagina",
];

/// The separators a name may contain but must not begin or end with.
const EDGE: [char; 3] = [' ', '_', '-'];

/// Top-level domains the address rule looks for.
const TLDS: [&str; 5] = [".de", ".com", ".net", ".org", ".io"];

/// Whitespace as JavaScript's `\s` understands it.
///
/// Unicode's White_Space property is the same set with one omission: `U+FEFF` is a format
/// character rather than whitespace, but `\s` matches it. A name the two implementations normalise
/// differently is a name stored as something other than what the user was shown accepting.
fn is_space(c: char) -> bool {
    c.is_whitespace() || c == '\u{feff}'
}

/// Trimmed, with runs of whitespace collapsed. What gets stored and displayed.
///
/// Surrounding whitespace is trimmed rather than refused — a name pasted with a trailing space is
/// a slip, not an attempt.
pub fn normalise(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    let mut gap = false;
    for c in raw.chars() {
        if is_space(c) {
            // Held rather than written, so a run at either end never reaches the string.
            gap = !out.is_empty();
            continue;
        }
        if gap {
            out.push(' ');
            gap = false;
        }
        out.push(c);
    }
    out
}

/// Folds leet substitutions before the word lists are applied, so `h1tl3r` and `f0tze` are caught
/// by the same entry as the plain spelling. Only substitutions that read as the letter: anything
/// more aggressive starts matching ordinary names.
pub fn fold(name: &str) -> String {
    name.to_lowercase()
        .chars()
        .map(|c| match c {
            '0' => 'o',
            '1' | '|' | '!' => 'i',
            '3' => 'e',
            '4' | '@' => 'a',
            '5' | '$' => 's',
            '7' => 't',
            '8' => 'b',
            other => other,
        })
        // Everything else goes, so separators cannot break a term up: `white power` folds to the
        // same string as `whitepower`.
        .filter(|c| c.is_ascii_lowercase() || matches!(c, 'ä' | 'ö' | 'ü' | 'ß'))
        .collect()
}

/// The rules, in the client's order. The order decides which refusal a caller gets back: `x`
/// twenty-one times is [`Refusal::TooLong`] and not four-in-a-row.
pub fn check(raw: &str) -> Result<(), Refusal> {
    let name = normalise(raw);

    let length = name.chars().count();
    if length < NAME_MIN {
        return Err(Refusal::TooShort);
    }
    if length > NAME_MAX {
        return Err(Refusal::TooLong);
    }

    // `is_alphabetic` is Unicode Alphabetic, marginally wider than the client's `\p{L}`: it also
    // admits combining marks. The two lists diverge only there, and only towards accepting what
    // the client would have refused — the harmless direction for a pre-filter a human reviews
    // behind.
    if !name
        .chars()
        .all(|c| c.is_alphabetic() || c.is_numeric() || EDGE.contains(&c))
    {
        return Err(Refusal::Shapeless);
    }

    let first = name.chars().next().expect("length was checked");
    let last = name.chars().next_back().expect("length was checked");
    if EDGE.contains(&first) || EDGE.contains(&last) {
        return Err(Refusal::Shapeless);
    }

    // Digits and separators only, e.g. `1488`, so numeric codes cannot slip past the letter rule.
    if name
        .chars()
        .all(|c| c.is_ascii_digit() || is_space(c) || c == '_' || c == '-')
    {
        return Err(Refusal::Shapeless);
    }

    if name.chars().filter(|c| c.is_alphabetic()).count() < 2 {
        return Err(Refusal::Shapeless);
    }

    let chars: Vec<char> = name.chars().collect();
    if chars.windows(4).any(|w| w.iter().all(|c| *c == w[0])) {
        return Err(Refusal::Shapeless);
    }

    let lower = name.to_lowercase();
    if RESERVED.contains(&lower.as_str()) {
        return Err(Refusal::Reserved);
    }

    if looks_like_an_address(&lower) {
        return Err(Refusal::Address);
    }

    let folded = fold(&name);
    if HATE.iter().any(|t| folded.contains(t)) {
        return Err(Refusal::Hate);
    }
    if VULGAR.iter().any(|t| folded.contains(t)) {
        return Err(Refusal::Vulgar);
    }

    Ok(())
}

/// A URL, an email, or a handle pretending to be one.
///
/// Unreachable in practice: the charset rule above has already refused `:` and `.`, so nothing
/// surviving to here can match. Ported anyway and kept in the client's position, because the day
/// the charset rule is loosened this is the only thing between the leaderboard and a row of
/// advertising.
fn looks_like_an_address(lower: &str) -> bool {
    if lower.contains("http:") || lower.contains("https:") || lower.contains("www.") {
        return true;
    }
    TLDS.iter().any(|tld| {
        lower.match_indices(tld).any(|(i, _)| {
            // The client's `\b`, which is ASCII word characters and nothing else.
            lower[i + tld.len()..]
                .chars()
                .next()
                .is_none_or(|c| !(c.is_ascii_alphanumeric() || c == '_'))
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ok(name: &str) {
        assert_eq!(check(name), Ok(()), "{name} should be accepted");
    }

    fn no(name: &str) {
        assert!(check(name).is_err(), "{name} should be refused");
    }

    #[test]
    fn accepts_ordinary_names() {
        ok("otherfren");
        ok("ganzfeld_enjoyer");
        ok("Monroe Institut");
        ok("Zoë");
        ok("remote-viewer-42");
        ok("Müller88");
        // Surrounding whitespace is trimmed rather than refused; only inner edges are an error.
        ok("otherfren ");
    }

    #[test]
    fn refuses_the_shapeless_ones() {
        no("");
        no("  ");
        no("ab");
        no(&"x".repeat(21));
        no("1488");
        no("_otherfren");
        no("otherfren_");
        no("-otherfren");
        no("aaaaa");
        no("a1");
        no("vril<script>");
        no("www.beispiel.de");
    }

    #[test]
    fn refuses_names_the_site_uses_for_itself() {
        no("admin");
        no("Moderator");
        no("vriltrainer");
        no("SYSTEM");
    }

    /// The board is public and the subject matter attracts exactly this, so it is worth asserting
    /// rather than assuming — including through the leet folding.
    #[test]
    fn refuses_hate_terms_spelled_straight_or_in_leet() {
        no("SiegHeil88");
        no("h1tl3r");
        no("Heil Hitler");
        no("white power");
        no("NSDAP_fan");
    }

    #[test]
    fn refuses_vulgarity() {
        no("Hurensohn");
        no("fuckyou");
        no("W1chser");
    }

    #[test]
    fn normalises_whitespace_before_storing() {
        assert_eq!(normalise("  Monroe   Institut "), "Monroe Institut");
    }

    /// The client picks its sentence from these distinctions, so collapsing them into one code
    /// would leave it unable to say what is wrong.
    #[test]
    fn the_refusal_names_the_rule() {
        assert_eq!(check("ab"), Err(Refusal::TooShort));
        assert_eq!(check(&"x".repeat(21)), Err(Refusal::TooLong));
        assert_eq!(check("admin"), Err(Refusal::Reserved));
        assert_eq!(check("h1tl3r"), Err(Refusal::Hate));
        assert_eq!(check("Hurensohn"), Err(Refusal::Vulgar));
        assert_eq!(check("_otherfren"), Err(Refusal::Shapeless));
    }

    /// The one rule that is not a list lookup, so worth pinning on its own.
    #[test]
    fn folding_reads_digits_as_the_letters_they_imitate() {
        assert_eq!(fold("H1tl3r"), "hitler");
        assert_eq!(fold("W1chs3r"), "wichser");
        assert_eq!(fold("f0tze"), "fotze");
        assert_eq!(fold("white power"), "whitepower");
        assert_eq!(fold("Müller88"), "müllerbb");
    }

    /// Reachable only if the charset rule is ever loosened, which is why it is still here.
    #[test]
    fn addresses_are_refused_once_they_can_reach_the_rule() {
        assert!(looks_like_an_address("www.beispiel.de"));
        assert!(looks_like_an_address("https://vriltrainer.de"));
        assert!(looks_like_an_address("otherfren.io"));
        assert!(!looks_like_an_address("otherfren"));
        // A word boundary, not a substring: `.department` is not a domain.
        assert!(!looks_like_an_address("x.department"));
    }
}
