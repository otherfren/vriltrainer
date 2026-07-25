import { RankBand } from './api.service';

/**
 * The ladder, and the argument.
 *
 * Two halves, deliberately separated. The **slugs** are the server's — they arrive in
 * `thresholds.bands` and in an account's `rank`, and they are what a band *is*. The titles and
 * the drawings are product copy and live here, one catalogue per domain (D10).
 *
 * The shares are not here at all. A band holds a share of the eligible population, that share is
 * configuration, and it is expected to move (D26, FR-050) — so the ladder is built from what the
 * server reported alongside the figures it is being drawn next to. A copy compiled into this file
 * would show a visitor edges that no longer decide anything.
 *
 * It is symmetric on purpose. Under chance the two ends are equally populated, so a Kartoffel is
 * exactly as rare as an Annunaki, and anyone who reaches the top should be able to see from the
 * shape of the ladder alone that somebody reached the bottom just as improbably.
 */
export interface RankDef {
  /**
   * The server's slug. `normie` is this client's name for "no band", which the API expresses by
   * leaving `rank` out — the middle 60 % has no band and needs no seat.
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
  { slug: 'kartoffel', title: $localize`:@@rank.kartoffel:Kartoffel`, icon: 'potato' },
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

/** One rung, with the share it holds and the population at which it starts existing. */
export interface Rung {
  rank: RankDef;
  /** The share of the eligible population, as the server reported it. */
  share: number;
  /**
   * The smallest eligible population at which this band exists at all: `ceil(1 / share)`.
   *
   * Mirrors `Thresholds::band_unlocks_at`. The band itself is never rounded up — it is awarded
   * once `share × eligible >= 1` — so a ladder drawn on a small site correctly shows most of its
   * rungs as not yet reachable (D23, FR-042).
   */
  unlocksAt: number;
  /** The middle rungs have no band and no seat; the ladder still shows where they sit. */
  middle: boolean;
}

/**
 * The whole ladder from the bands the server published, best first.
 *
 * Normie's share is what is left after both ends, computed rather than stated: the API publishes
 * the bands, and the middle is by definition everything they do not cover. Writing "60 %" here
 * would be a number that stops being true the first time a share moves.
 */
export function ladder(bands: RankBand[]): Rung[] {
  const covered = bands.reduce((sum, b) => sum + b.share, 0);
  const highs = bands.map((b) => rung(rankFor(b.high), b.share));
  const lows = [...bands].reverse().map((b) => rung(rankFor(b.low), b.share));
  const middle = { ...rung(NORMIE, Math.max(0, 1 - 2 * covered)), middle: true };
  return [...highs, middle, ...lows];
}

function rung(rank: RankDef, share: number): Rung {
  return { rank, share, unlocksAt: share > 0 ? Math.ceil(1 / share) : 0, middle: false };
}
