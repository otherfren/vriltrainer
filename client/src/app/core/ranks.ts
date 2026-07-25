export interface RankDef {
  title: string;
  icon: string;
  /** The share of players this band holds. */
  span: string;
  /** The band's lower edge in standard deviations from chance, and its drawn centre. */
  z: number;
}

/**
 * The ladder, and the argument.
 *
 * Bands are shares, not seats: "the best 0,1 %" means the same thing whether nine people are
 * playing or nine hundred thousand, which is the only way a rank keeps its meaning as the site
 * grows. The edges are the values a fair coin predicts, so a real effect would show up as the
 * top bands holding more than their share — that is what makes the chart worth drawing.
 *
 * It is symmetric on purpose. Under chance the two ends are equally populated, so a Kartoffel is
 * exactly as rare as an Annunaki, and anyone who reaches the top should be able to see from the
 * shape of the ladder alone that somebody reached the bottom just as improbably.
 */
export const RANKS: RankDef[] = [
  { title: 'Annunaki', icon: 'annunaki', span: 'beste 0,1 %', z: 3.3 },
  { title: 'Insektoider Loosh-Farmer', icon: 'loosh', span: 'beste 0,5 %', z: 2.8 },
  { title: 'Reptiloidenarchont', icon: 'reptilian', span: 'beste 2 %', z: 2.25 },
  { title: 'Grey Alien', icon: 'grey', span: 'beste 7 %', z: 1.7 },
  { title: 'Psionisches Asset', icon: 'asset', span: 'beste 20 %', z: 1.1 },
  { title: 'Normie', icon: 'normie', span: 'die mittleren 60 %', z: 0 },
  { title: 'Zirbeldrüse verkalkt', icon: 'pineal', span: 'unterste 20 %', z: -1.1 },
  { title: 'Erdstrahlen-Opfer', icon: 'erdstrahlen', span: 'unterste 7 %', z: -1.7 },
  { title: 'Orgonit-Enjoyer', icon: 'orgonit', span: 'unterste 2 %', z: -2.25 },
  { title: 'Psi-Nullleiter', icon: 'nullleiter', span: 'unterste 0,5 %', z: -2.8 },
  { title: 'Kartoffel', icon: 'potato', span: 'unterste 0,1 %', z: -3.3 },
];

const BY_TITLE = new Map(RANKS.map((r) => [r.title, r]));

export function rankIcon(iconOrTitle: string): string {
  const byTitle = BY_TITLE.get(iconOrTitle);
  return `rank/rank-${byTitle ? byTitle.icon : iconOrTitle}.svg`;
}

/**
 * The rank a percentile holds, where 0 is the very top of the board.
 *
 * Taking a share rather than a seat count is the whole point: place 3 of 10 is nothing, place 3
 * of 200 000 is the top 0,0015 %, and only one of those deserves a title.
 */
export function rankForPercentile(topFraction: number): RankDef {
  if (topFraction <= 0.001) return RANKS[0];
  if (topFraction <= 0.005) return RANKS[1];
  if (topFraction <= 0.02) return RANKS[2];
  if (topFraction <= 0.07) return RANKS[3];
  if (topFraction <= 0.2) return RANKS[4];
  if (topFraction < 0.8) return RANKS[5];
  if (topFraction < 0.93) return RANKS[6];
  if (topFraction < 0.98) return RANKS[7];
  if (topFraction < 0.995) return RANKS[8];
  if (topFraction < 0.999) return RANKS[9];
  return RANKS[10];
}
