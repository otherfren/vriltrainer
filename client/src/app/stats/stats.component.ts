import { Component, inject } from '@angular/core';
import { VrilMeterComponent } from '../core/vril-meter.component';
import { RANKS, RankDef, rankIcon } from '../core/ranks';
import { PlayerService, STATS_UNLOCK_AT } from '../core/player.service';

interface Bin {
  rank: RankDef;
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

  /**
   * Players by measured deviation, one column per rank band. The band edges are fixed at the
   * values chance predicts, so these counts are a finding rather than a definition: an effect
   * would show up as the right-hand columns holding more than their share.
   *
   * Ascending, so the chart reads left to right the way a number line does.
   */
  private readonly counts = [1, 3, 12, 34, 96, 430, 92, 38, 10, 3, 1];

  readonly bins: Bin[] = [...RANKS]
    .reverse()
    .map((rank, i) => ({ rank, count: this.counts[i] }));

  readonly players = this.counts.reduce((a, b) => a + b, 0);

  /**
   * The two ends, summed from the bars rather than typed in beside them. Under chance they are
   * equally populated and that is the whole test, so they had better be the same numbers.
   */
  readonly tailHigh = this.bins.filter((b) => b.rank.z >= 2).reduce((a, b) => a + b.count, 0);
  readonly tailLow = this.bins.filter((b) => b.rank.z <= -2).reduce((a, b) => a + b.count, 0);

  /** What a pure coin would put past 2σ at each end: the one-sided normal tail, 2,275 %. */
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

  get peak(): number {
    return Math.max(...this.bins.map((b) => b.count));
  }

  /** Square-root scaling. Linear against a peak of 402 crushes the tails to invisibility, and
   *  the tails are the entire argument: under chance they are equally populated. */
  height(c: number): number {
    return Math.max(6, (Math.sqrt(c) / Math.sqrt(this.peak)) * 100);
  }
}
