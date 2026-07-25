import { Injectable, computed, signal } from '@angular/core';

/** Abramowitz & Stegun 7.1.26 — enough precision for a figure rounded to whole people. */
function normalCdf(z: number): number {
  const sign = z < 0 ? -1 : 1;
  const x = Math.abs(z) / Math.SQRT2;
  const t = 1 / (1 + 0.3275911 * x);
  const y =
    1 -
    ((((1.061405429 * t - 1.453152027) * t + 1.421413741) * t - 0.284496736) * t + 0.254829592) *
      t *
      Math.exp(-x * x);
  return 0.5 * (1 + sign * y);
}

/** Chance itself: one target out of eight. Everything on the site is a displacement from this. */
export const CHANCE = 1 / 8;

/** Statistics stay closed until there are enough trials for a rate to mean anything. */
export const STATS_UNLOCK_AT = 10;

/** Ranks are positions, not thresholds — you are not ranked until the board can rank you. */
export const RANKABLE_AT = 100;

export interface Rank {
  title: string;
  icon: string;
  note: string;
}

/**
 * What this browser has done so far.
 *
 * Held in one place because two very different surfaces need it: the trial screen, and the status
 * panel that sits under every page. Demo numbers for now — the server owns these once it exists.
 */
@Injectable({ providedIn: 'root' })
export class PlayerService {
  readonly completed = signal(7);
  readonly hits = signal(1);

  readonly unlocked = computed(() => this.completed() >= STATS_UNLOCK_AT);
  readonly remaining = computed(() => Math.max(0, STATS_UNLOCK_AT - this.completed()));

  readonly rate = computed(() => (this.completed() === 0 ? 0 : this.hits() / this.completed()));

  /** Standard deviations from chance, on the binomial's own scale. */
  readonly z = computed(() => {
    const n = this.completed();
    if (n === 0) return 0;
    return (this.rate() - CHANCE) / Math.sqrt((CHANCE * (1 - CHANCE)) / n);
  });

  /**
   * Wilson lower bound at 95 %. The honest headline number: it weighs how many trials produced
   * the rate, so three lucky hits out of eight do not outrank a steady run of four hundred.
   */
  readonly wilson = computed(() => {
    const n = this.completed();
    if (n === 0) return 0;
    const z = 1.959964;
    const p = this.rate();
    const d = 1 + (z * z) / n;
    const centre = p + (z * z) / (2 * n);
    const spread = z * Math.sqrt((p * (1 - p)) / n + (z * z) / (4 * n * n));
    return Math.max(0, (centre - spread) / d);
  });

  /**
   * How many of 10 000 people with no ability at all would reach this rate by luck.
   *
   * The single most important number on the page: a rate without it is not interpretable,
   * and it is almost always large enough to explain the rate on its own.
   */
  readonly perTenK = computed(() => Math.round(10000 * (1 - normalCdf(this.z()))));

  /**
   * The rank this browser actually holds.
   *
   * Almost always Normie, and that is the joke, but it is also the truth: the ladder hands out
   * places, and you do not get a place until you are eligible for the board at all.
   */
  readonly rank = computed<Rank>(() => {
    if (this.completed() < RANKABLE_AT) {
      return {
        title: 'Normie',
        icon: 'normie',
        note: `Ranglistenfähig ab ${RANKABLE_AT} Sitzungen an mindestens drei verschiedenen Tagen.`,
      };
    }
    if (this.z() <= -2) {
      return {
        title: 'Kartoffel',
        icon: 'potato',
        note: 'Deutlich unter dem Zufall. Genauso aussagekräftig wie deutlich darüber.',
      };
    }
    return {
      title: 'Normie',
      icon: 'normie',
      note: 'Ränge sind Plätze, keine Schwellen — die Rangliste vergibt sie, nicht deine Quote.',
    };
  });

  record(hit: boolean): void {
    this.completed.update((n) => n + 1);
    if (hit) this.hits.update((n) => n + 1);
  }
}
