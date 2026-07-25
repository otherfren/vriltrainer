import { Component, Input } from '@angular/core';

/**
 * The signature element: a power meter with an outrageously ambitious scale, in which every
 * player who has ever played sits inside the first segment.
 *
 * It carries exactly what a figure on this site carries — displacement from the 12.5 % chance
 * rate — and it appears on every page because a rate shown without its baseline is not
 * interpretable. The arcade framing is not decoration: a bar that refuses to fill states the
 * finding faster than a sentence about standard deviations does.
 *
 * The mapping is deliberately generous. Four standard deviations, which is a once-in-a-fleet
 * outlier, buys three cells out of twenty-eight. Nobody gets out of NORMAL.
 */
@Component({
  selector: 'app-vril-meter',
  standalone: true,
  template: `
    <div class="meter" role="img" [attr.aria-label]="label + ': ' + reading">
      <div class="meter__head">
        <span class="meter__label">{{ label }}</span>
        <span class="meter__reading">{{ reading }}</span>
      </div>

      <div class="meter__scale" aria-hidden="true">
        @for (s of stops; track s; let i = $index) {
          <span class="meter__stop" [class.meter__stop--reached]="i === 0">{{ s }}</span>
        }
      </div>

      <div class="meter__bar" aria-hidden="true">
        @for (c of cells; track c) {
          <span
            class="meter__cell"
            [class.meter__cell--lit]="c <= needleCell"
            [class.meter__cell--mine]="mine && c <= needleCell"
          ></span>
        }
      </div>

      <div class="meter__foot" aria-hidden="true">
        <span class="meter__needle" [style.left.%]="needlePercent">{{ needleLabel }}</span>
      </div>
    </div>
  `,
})
export class VrilMeterComponent {
  /** Standard deviations from chance. Positive is above the line. */
  @Input() deviation = 0;
  @Input() label = 'Vrilpegel';
  @Input() reading = '';
  /** Draws the fill in the player's colour rather than the aggregate's. */
  @Input() mine = false;
  @Input() needleLabel = '';

  readonly stops = ['Normal', 'Warm', 'Heiss', 'Vril'];
  readonly cells = Array.from({ length: 28 }, (_, i) => i);

  /** The centre of the first scale stop, which is where chance itself sits. */
  private readonly zeroCell = 3;

  get needleCell(): number {
    const clamped = Math.max(-4, Math.min(4, this.deviation));
    return Math.max(0, Math.round(this.zeroCell + clamped * 0.75));
  }

  get needlePercent(): number {
    return ((this.needleCell + 0.5) / this.cells.length) * 100;
  }
}
