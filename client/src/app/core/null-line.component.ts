import { Component, Input } from '@angular/core';

/**
 * The signature element: a real horizontal rule standing for the 12.5% chance rate, with a
 * marker displaced from it by the measured deviation.
 *
 * It appears on every page because the point of the product is that almost everything sits on
 * this line. A figure shown without its baseline is not interpretable, and this makes the
 * baseline impossible to overlook.
 */
@Component({
  selector: 'app-null-line',
  standalone: true,
  template: `
    <div class="nullline" role="img" [attr.aria-label]="label + ': ' + reading">
      <div class="nullline__axis"></div>
      <span class="nullline__label">{{ label }}</span>
      <span
        class="nullline__marker"
        [class.nullline__marker--you]="mine"
        [style.left.%]="position"
        [style.marginTop.px]="offsetPx"
      ></span>
      <span class="nullline__reading">{{ reading }}</span>
    </div>
  `,
})
export class NullLineComponent {
  /** Standard deviations from chance. Positive is above the line. */
  @Input() deviation = 0;
  @Input() label = 'Nulllinie';
  @Input() reading = '';
  @Input() mine = false;
  /** Horizontal placement carries no meaning; it only keeps markers from overlapping. */
  @Input() position = 50;

  /** Vertical displacement, clamped so an extreme outlier stays inside the strip. */
  get offsetPx(): number {
    const clamped = Math.max(-4, Math.min(4, this.deviation));
    return -clamped * 5;
  }
}
