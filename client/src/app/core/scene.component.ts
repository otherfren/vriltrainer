import { Component } from '@angular/core';

/**
 * The horizon the game is played on: two moons, a starfield, a tiled alien ridgeline, and
 * the cast of the rank ladder standing behind it.
 *
 * The characters are the ranks — the mantid archon, the reptilian, the Grey, and the saucer
 * that the Flugscheibenpilot flies — so the background is not decoration bolted on, it is the
 * ladder you are climbing, watching you fail to climb it.
 *
 * It sits behind everything and is inert: fixed, aria-hidden, pointer-events none. All motion
 * is transform-only, and every actor's resting position is its CSS position rather than an
 * animation keyframe, so with reduced motion the scene simply stops instead of emptying out.
 */
@Component({
  selector: 'app-scene',
  standalone: true,
  template: `
    <div class="scene" aria-hidden="true">
      <div class="scene__sky">
        @for (s of stars; track $index) {
          <span
            class="scene__star"
            [class.scene__star--blink]="s.blink"
            [style.left.%]="s.x"
            [style.top.%]="s.y"
            [style.width.px]="s.size"
            [style.height.px]="s.size"
            [style.animation-delay.s]="s.delay"
          ></span>
        }

        <img class="scene__moon scene__moon--big" src="scene/moon-big.svg" alt="" />
        <img class="scene__moon scene__moon--small" src="scene/moon-small.svg" alt="" />
        <img class="scene__saucer" src="rank/rank-pilot.svg" alt="" />
      </div>

      <div class="scene__ridge scene__ridge--far"></div>

      <img class="scene__peek scene__peek--archon" src="rank/rank-archon.svg" alt="" />
      <img class="scene__peek scene__peek--grey" src="rank/rank-grey.svg" alt="" />
      <img class="scene__peek scene__peek--reptilian" src="rank/rank-reptilian.svg" alt="" />

      <div class="scene__ridge scene__ridge--near"></div>
    </div>
  `,
  styleUrl: './scene.component.scss',
})
export class SceneComponent {
  /** Fixed positions rather than random ones, so the sky does not reshuffle on every render. */
  readonly stars = SceneComponent.scatter(56);

  private static scatter(count: number) {
    // A small LCG: the same sky every load, on every machine, without shipping a data file.
    let seed = 0x5f3a21;
    const next = () => ((seed = (seed * 1664525 + 1013904223) >>> 0) / 4294967296);

    return Array.from({ length: count }, () => {
      const roll = next();
      return {
        x: next() * 100,
        // Kept out of the lower third, where the ridges and the page's own content are.
        y: next() * 62,
        size: roll < 0.75 ? 2 : 3,
        blink: roll > 0.82,
        delay: next() * 6,
      };
    });
  }
}
