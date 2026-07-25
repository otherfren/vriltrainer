export interface RankDef {
  title: string;
  icon: string;
  /** Which places hold this rank. Not a threshold — the board hands out positions. */
  span: string;
  /** Where the rank sits on the distribution, in standard deviations from chance. */
  z: number;
}

/**
 * The ladder, and the argument.
 *
 * It is symmetric on purpose. Under pure chance the two ends of the distribution are equally
 * populated, so there are exactly as many places below Normie as above it, and a Kartoffel is
 * exactly as rare as an Annunaki. Anyone who reaches the top of this ladder should be able to
 * see, from the shape of the ladder alone, that somebody reached the bottom just as improbably.
 */
export const RANKS: RankDef[] = [
  { title: 'Annunaki', icon: 'annunaki', span: '1–3', z: 3.0 },
  { title: 'Insektoider Archont', icon: 'archon', span: '4–10', z: 2.3 },
  { title: 'Reptiloidenarchont', icon: 'reptilian', span: '11–30', z: 1.7 },
  { title: 'Grey Alien', icon: 'grey', span: '31–80', z: 1.2 },
  { title: 'Flugscheibenpilot', icon: 'pilot', span: '81–200', z: 0.7 },
  { title: 'Normie', icon: 'normie', span: 'alles dazwischen', z: 0.0 },
  { title: 'Zirbeldrüse verkalkt', icon: 'pineal', span: '81–200 von unten', z: -0.7 },
  { title: 'Erdstrahlen-Opfer', icon: 'erdstrahlen', span: '31–80 von unten', z: -1.2 },
  { title: 'Aluhut verkehrt herum', icon: 'aluhut', span: '11–30 von unten', z: -1.7 },
  { title: 'Psi-Nullleiter', icon: 'nullleiter', span: '4–10 von unten', z: -2.3 },
  { title: 'Kartoffel', icon: 'potato', span: 'die letzten 3', z: -3.0 },
];

const BY_TITLE = new Map(RANKS.map((r) => [r.title, r]));

export function rankIcon(iconOrTitle: string): string {
  const byTitle = BY_TITLE.get(iconOrTitle);
  return `rank/rank-${byTitle ? byTitle.icon : iconOrTitle}.svg`;
}

/** The rank held by a given place on the board. */
export function rankForPlace(place: number): RankDef {
  if (place <= 3) return RANKS[0];
  if (place <= 10) return RANKS[1];
  if (place <= 30) return RANKS[2];
  if (place <= 80) return RANKS[3];
  if (place <= 200) return RANKS[4];
  return RANKS[5];
}
