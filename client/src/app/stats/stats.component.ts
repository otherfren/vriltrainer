import { Component, LOCALE_ID, computed, inject, signal } from '@angular/core';
import { VrilMeterComponent } from '../core/vril-meter.component';
import { AggregateStats, ApiService } from '../core/api.service';
import { Rung, ladder, rankIcon } from '../core/ranks';
import { PlayerService } from '../core/player.service';
import { SessionService } from '../core/session.service';

/**
 * The headline finding, from `GET /api/stats/aggregate`.
 *
 * The page used to state 148 213 trials and a distribution of 720 players across eleven bands.
 * None of it existed, and inventing the exact numbers a site of this kind would love to have is
 * the one thing it cannot do and remain worth reading. All of it is gone.
 *
 * What replaces the invented histogram is not a smaller invented histogram. The API publishes two
 * tail counts and not a per-band distribution, so the chart is the two tails — which is also
 * exactly the test FR-043 asks a reader to perform by looking: under chance they arrive in
 * roughly equal numbers, and a real effect can only make the upper one heavier.
 *
 * The empty state is an empty grey chart and one honest line (T105). Not a spinner that never
 * resolves, and not a zero dressed up as a finding.
 */
@Component({
  selector: 'app-stats',
  standalone: true,
  imports: [VrilMeterComponent],
  templateUrl: './stats.component.html',
  styleUrl: './stats.component.scss',
})
export class StatsComponent {
  private readonly api = inject(ApiService);
  /** Set by the localized build, so decimals follow the domain rather than being German. */
  private readonly locale = inject(LOCALE_ID);

  /** Your own figures come from the one place that counts them, so the panel under every page and
   *  this section can never disagree about how many trials you have played. */
  readonly player = inject(PlayerService);
  readonly session = inject(SessionService);

  readonly aggregate = signal<AggregateStats | null>(null);
  /**
   * Paired with `aggregate()` rather than joined by a third "in flight" flag: the template has to
   * tell apart figures, a failure, and neither — and neither *is* the request being in flight.
   */
  readonly failed = signal(false);

  /** No trial has been completed anywhere yet. The expected state at launch, and for a while. */
  readonly empty = computed(() => (this.aggregate()?.trials ?? 0) === 0);

  readonly tailHigh = computed(() => this.aggregate()?.tail_high ?? 0);
  readonly tailLow = computed(() => this.aggregate()?.tail_low ?? 0);
  readonly tailSigma = computed(() => this.aggregate()?.tail_sigma ?? 2);
  readonly tailMinTrials = computed(() => this.aggregate()?.tail_min_trials ?? 0);

  /** The ladder, built from the shares the server reported rather than from a copy here (D26). */
  readonly rungs = computed<Rung[]>(() => {
    const t = this.aggregate()?.thresholds;
    return t === undefined ? [] : ladder(t.bands);
  });

  readonly icon = rankIcon;

  constructor() {
    void this.load();
  }

  async load(): Promise<void> {
    this.failed.set(false);
    try {
      this.aggregate.set(await this.api.aggregate());
    } catch {
      this.failed.set(true);
    }
  }

  private de(n: number, digits = 2): string {
    return n.toLocaleString(this.locale, {
      minimumFractionDigits: digits,
      maximumFractionDigits: digits,
    });
  }

  count(n: number): string {
    return n.toLocaleString(this.locale);
  }

  pct(fraction: number, digits = 2): string {
    return this.de(fraction * 100, digits);
  }

  signed(n: number): string {
    return (n >= 0 ? '+' : '') + this.de(n);
  }

  share(fraction: number): string {
    return this.de(fraction * 100, fraction < 0.01 ? 1 : 0);
  }

  /** Chance is 1 in 8 by construction (D3), written as a figure so it follows the locale. */
  readonly chanceRate = this.pct(0.125, 1);

  /**
   * Strings the template can only reach through a binding, which the extractor cannot see. Both
   * were built by concatenating German fragments in the template before.
   */
  readonly tailsAriaLabel = () =>
    $localize`:@@stats.tails.aria:${this.tailHigh()}:high: Konten über +${this.tailSigma()}:sigma:σ, ${this.tailLow()}:low: darunter`;
  readonly mineReading = () =>
    $localize`:@@stats.mine.reading:${this.mineZ()}:deviation: σ · gesicherte Mindestquote ${this.mineWilson()}:wilson: %`;

  minePct = () => this.pct(this.player.rate(), 1);
  mineZ = () => this.signed(this.player.deviation());
  mineWilson = () => this.pct(this.player.wilson(), 1);
  perTenK = () => this.count(this.player.perTenK());

  /**
   * Bar height for a tail count, against the larger of the two.
   *
   * Against each other rather than against a fixed scale, because the comparison the chart is
   * making is between the two ends and nothing else. A minimum height keeps a count of zero
   * visible as a drawn bar of nothing rather than as a missing column.
   */
  height(n: number): number {
    const peak = Math.max(this.tailHigh(), this.tailLow(), 1);
    return Math.max(4, (n / peak) * 100);
  }
}
