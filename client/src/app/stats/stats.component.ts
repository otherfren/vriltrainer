import { Component, inject } from '@angular/core';
import { VrilMeterComponent } from '../core/vril-meter.component';
import { RANKS, rankIcon } from '../core/ranks';
import { PlayerService, STATS_UNLOCK_AT } from '../core/player.service';

interface Bin {
  z: number;
  count: number;
}

@Component({
  selector: 'app-stats',
  standalone: true,
  imports: [VrilMeterComponent],
  templateUrl: './stats.component.html',
  styleUrl: './stats.component.scss',
})
export class StatsComponent {
  readonly trials = 148213;
  readonly rate = 12.53;
  readonly expected = 12.5;
  readonly aggregateZ = 0.31;

  readonly ranks = RANKS;
  readonly icon = rankIcon;

  /** Participants by deviation. The symmetry of the shape is the argument. */
  readonly bins: Bin[] = [
    { z: -3, count: 1 },
    { z: -2, count: 12 },
    { z: -1, count: 141 },
    { z: 0, count: 402 },
    { z: 1, count: 148 },
    { z: 2, count: 14 },
    { z: 3, count: 2 },
  ];

  readonly players = this.bins.reduce((a, b) => a + b.count, 0);

  /**
   * The two tails, read off the bins rather than typed in beside them. Under chance they are
   * equally populated and that is the whole test, so they had better be the same numbers the
   * bars are drawn from.
   */
  readonly tailHigh = this.bins.filter((b) => b.z >= 2).reduce((a, b) => a + b.count, 0);
  readonly tailLow = this.bins.filter((b) => b.z <= -2).reduce((a, b) => a + b.count, 0);

  /** How many players a pure coin would push past 2σ: the one-sided normal tail, 2,275 %. */
  readonly expectedTail = Math.round(this.players * 0.02275);

  /** Your own figures come from the one place that counts them, so the panel under every
   *  page and this section can never disagree about how many trials you have played. */
  readonly player = inject(PlayerService);
  readonly unlockAt = STATS_UNLOCK_AT;

  private de(n: number, digits = 2): string {
    return n.toLocaleString('de-DE', {
      minimumFractionDigits: digits,
      maximumFractionDigits: digits,
    });
  }

  readonly trialsLabel = this.trials.toLocaleString('de-DE');
  readonly playersLabel = this.players.toLocaleString('de-DE');
  readonly rateLabel = this.de(this.rate);
  readonly expectedLabel = this.de(this.expected, 1);
  readonly aggregateZLabel = this.de(this.aggregateZ);
  mineRateLabel = () => this.de(this.player.rate() * 100, 1);
  mineZLabel = () => (this.player.z() >= 0 ? '+' : '') + this.de(this.player.z());
  mineWilsonLabel = () => this.de(this.player.wilson() * 100, 1);
  perTenKLabel = () => this.player.perTenK().toLocaleString('de-DE');

  /**
   * Where a rank sits above the histogram, in percent of its width.
   *
   * The seven bins are equal flex columns for z = -3 … +3, so bin i's centre is at (i + 0.5)/7.
   * Solving for z puts every rank on the same axis as the bars rather than near them, which is
   * the entire point of drawing them together.
   */
  rankLeft(z: number): number {
    return ((z + 3) * 100) / 7 + 50 / 7;
  }

  get peak(): number {
    return Math.max(...this.bins.map((b) => b.count));
  }

  /** Square-root scaling. Linear against a peak of 402 crushes the tails to invisibility, and
   *  the tails are the entire argument: under chance they are equally populated. */
  height(c: number): number {
    return Math.max(6, (Math.sqrt(c) / Math.sqrt(this.peak)) * 100);
  }
}
