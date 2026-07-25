import { Component } from '@angular/core';
import { VrilMeterComponent } from '../core/vril-meter.component';

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

  /** The two tails. Under chance they are equally populated, and that is the whole test. */
  readonly tailHigh = 7;
  readonly tailLow = 6;

  readonly mine = { completed: 120, hits: 21, rate: 17.5, z: 1.62, perTenK: 527, wilson: 11.7 };

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

  private de(n: number, digits = 2): string {
    return n.toLocaleString('de-DE', { minimumFractionDigits: digits, maximumFractionDigits: digits });
  }

  readonly trialsLabel = this.trials.toLocaleString('de-DE');
  readonly rateLabel = this.de(this.rate);
  readonly expectedLabel = this.de(this.expected, 1);
  readonly aggregateZLabel = this.de(this.aggregateZ);
  readonly mineRateLabel = this.de(this.mine.rate, 1);
  readonly mineZLabel = this.de(this.mine.z);
  readonly mineWilsonLabel = this.de(this.mine.wilson, 1);
  readonly perTenKLabel = this.mine.perTenK.toLocaleString('de-DE');

  get peak(): number {
    return Math.max(...this.bins.map((b) => b.count));
  }

  /** Square-root scaling. Linear against a peak of 402 crushes the tails to invisibility, and
   *  the tails are the entire argument: under chance they are equally populated. */
  height(c: number): number {
    return Math.max(6, (Math.sqrt(c) / Math.sqrt(this.peak)) * 100);
  }
}
