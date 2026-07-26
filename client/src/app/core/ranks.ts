import { RankBand } from './api.service';

/**
 * The ladder, and the argument.
 *
 * Two halves, deliberately separated. The **slugs** are the server's — they arrive in
 * `thresholds.bands` and in an account's `rank`, and they are what a band *is*. The titles and
 * the drawings are product copy and live here, one catalogue per domain (D10).
 *
 * The edges are not here at all. A band starts at a distance from chance, that distance is
 * configuration, and it is expected to move (D26, FR-050) — so the ladder is built from what the
 * server reported alongside the figures it is being drawn next to. A copy compiled into this file
 * would show a visitor edges that no longer decide anything.
 *
 * It is symmetric on purpose. Under chance the two ends are equally populated, so a Kartoffel is
 * exactly as rare as an Annunaki, and anyone who reaches the top should be able to see from the
 * shape of the ladder alone that somebody reached the bottom just as improbably. Since D31 that is
 * a claim the page can be checked against rather than a definition: the rungs are cut at fixed
 * sigmas, so how many people are actually standing on each is a measurement.
 */
export interface RankDef {
  /**
   * The server's slug. `normie` is this client's name for "no band", which the API expresses by
   * leaving `rank` out — the middle rung has no band to name.
   */
  slug: string;
  title: string;
  /**
   * The drawing, at `rank/rank-<icon>.svg`. Not always the slug: the file predates the server and
   * is called `potato`, while the band is `kartoffel`.
   */
  icon: string;
}

export const NORMIE: RankDef = { slug: 'normie', title: $localize`:@@rank.normie:Normie`, icon: 'normie' };

const DEFS: RankDef[] = [
  { slug: 'annunaki', title: $localize`:@@rank.annunaki:Annunaki`, icon: 'annunaki' },
  { slug: 'loosh', title: $localize`:@@rank.loosh:Insektoider Loosh-Farmer`, icon: 'loosh' },
  { slug: 'reptilian', title: $localize`:@@rank.reptilian:Reptiloidenarchont`, icon: 'reptilian' },
  { slug: 'grey', title: $localize`:@@rank.grey:Grey Alien`, icon: 'grey' },
  { slug: 'asset', title: $localize`:@@rank.asset:Psionisches Asset`, icon: 'asset' },
  NORMIE,
  { slug: 'pineal', title: $localize`:@@rank.pineal:Zirbeldrüse verkalkt`, icon: 'pineal' },
  { slug: 'erdstrahlen', title: $localize`:@@rank.erdstrahlen:Erdstrahlen-Opfer`, icon: 'erdstrahlen' },
  { slug: 'orgonit', title: $localize`:@@rank.orgonit:Orgonit-Enjoyer`, icon: 'orgonit' },
  { slug: 'nullleiter', title: $localize`:@@rank.nullleiter:Psi-Nullleiter`, icon: 'nullleiter' },
  { slug: 'kartoffel', title: $localize`:@@rank.kartoffel:Geimpfte Kartoffel`, icon: 'potato' },
];

const BY_SLUG = new Map(DEFS.map((r) => [r.slug, r]));

/**
 * The band a slug names. An absent slug is Normie — the honest answer for almost everyone, and
 * what the API means when it omits `rank`.
 *
 * A slug this catalogue does not know falls back to its own text rather than to Normie. An
 * operator who adds a band to the configuration should see it appear untranslated, not see
 * everybody in it silently demoted to the middle.
 */
export function rankFor(slug: string | null | undefined): RankDef {
  if (slug === null || slug === undefined || slug === '') return NORMIE;
  return BY_SLUG.get(slug) ?? { slug, title: slug, icon: 'normie' };
}

export function rankIcon(iconOrSlug: string): string {
  const def = BY_SLUG.get(iconOrSlug);
  return `rank/rank-${def ? def.icon : iconOrSlug}.svg`;
}

/** One rung, with the stretch of sigma it covers and how much of a chance population lands there. */
export interface Rung {
  rank: RankDef;
  /** Where the rung starts, as an absolute distance from chance. Zero for the middle. */
  from: number;
  /** Where it ends, or null on the two open-ended rungs — nine sigma is still an Annunaki. */
  to: number | null;
  /**
   * The share of a pure-guessing population that lands on this rung.
   *
   * Computed here from the normal distribution rather than reported, because it is not a setting:
   * it is what the configured edges *imply* under the null hypothesis, and that is the number worth
   * printing beside a title. Under the share model this figure was the configuration, which is why
   * the ladder could not say anything — it stated its own definition back.
   */
  chanceShare: number;
  /** The middle rung has no band and no slug; the ladder still shows where it sits. */
  middle: boolean;
}

/**
 * The whole ladder from the bands the server published, best first.
 *
 * The middle is derived, not stated: it is everything the two ends do not cover, which for a
 * Normie band of ±0,3 σ is about a quarter of a chance population.
 */
export function ladder(bands: RankBand[]): Rung[] {
  if (bands.length === 0) return [];
  // Best first, so a rung ends where the rung above it begins; the best one does not end.
  const highs = bands.map((b, i) =>
    rung(rankFor(b.high), b.from_sigma, i === 0 ? null : bands[i - 1].from_sigma),
  );
  const lows = [...bands]
    .reverse()
    .map((b, i, all) => rung(rankFor(b.low), b.from_sigma, i === all.length - 1 ? null : all[i + 1].from_sigma));
  const innermost = bands[bands.length - 1].from_sigma;
  const middle: Rung = {
    rank: NORMIE,
    from: 0,
    to: innermost,
    chanceShare: 2 * phi(innermost) - 1,
    middle: true,
  };
  return [...highs, middle, ...lows];
}

function rung(rank: RankDef, from: number, to: number | null): Rung {
  return { rank, from, to, chanceShare: chanceShare(from, to), middle: false };
}

/** How much of a chance population sits between `from` and `to` sigma, on one side. */
function chanceShare(from: number, to: number | null): number {
  const outer = to === null ? 1 : phi(to);
  return Math.max(0, outer - phi(from));
}

/**
 * The standard normal CDF.
 *
 * Abramowitz & Stegun 7.1.26 for the error function, whose absolute error is under 1,5·10⁻⁷ — three
 * orders of magnitude finer than the rarest rung it has to describe, and this is display copy
 * rather than anything a rank is decided by. Nothing on this page is *awarded* from this number:
 * the server decides the band from the account's own sigma, and this only says how often luck alone
 * produces one.
 */
function phi(z: number): number {
  return 0.5 * (1 + erf(z / Math.SQRT2));
}

function erf(x: number): number {
  const sign = x < 0 ? -1 : 1;
  const t = 1 / (1 + 0.3275911 * Math.abs(x));
  const poly =
    t * (0.254829592 + t * (-0.284496736 + t * (1.421413741 + t * (-1.453152027 + t * 1.061405429))));
  return sign * (1 - poly * Math.exp(-x * x));
}
