import { Component, LOCALE_ID, computed, inject } from '@angular/core';
import { RouterLink } from '@angular/router';
import { PlayerService } from './player.service';
import { SessionService } from './session.service';

/** Chance is 1 in 8 by construction (D3). Eight images, one of them right. */
const CHANCE = 0.125;

/**
 * Where you stand, under every page.
 *
 * It used to be the aggregate meter, which said the same thing on every page forever and was
 * about everyone rather than you. This says one useful thing at a time: before the unlock, how
 * far you are from a number that means anything; after it, the four figures somebody actually
 * came for — the rank, the hits, the hits chance would have handed out anyway, and how often luck
 * alone gets this far. The middle pair is counts rather than percentages on purpose: the surplus
 * is then a subtraction, not a statistic.
 *
 * Nothing is folded away behind a toggle any more. A panel with a "show details" button is a panel
 * that has decided what it shows is not the interesting part, and the fix for that is to show the
 * interesting part, not to add a click. The figures that were behind it — deviation, the Wilson
 * floor, the abandoned count, the meter — are on the statistics page, which the heading links to.
 *
 * Every figure comes from `GET /api/stats/me`. Two states exist only because it does: the first
 * load, where there is nothing to draw yet, and the failure, where there is nothing to draw and a
 * reason. Drawing zeroes in either would be a claim about the account that nobody made.
 */
@Component({
  selector: 'app-status-panel',
  standalone: true,
  imports: [RouterLink],
  template: `
    @if (session.signedIn()) {
      <div class="status panel">
        @if (!player.loaded()) {
          <div class="status__head">
            <p class="eyebrow" i18n="@@status.heading">Dein Stand</p>
          </div>
          @if (player.failed()) {
            <p class="status__why" i18n="@@status.unavailable">
              Die Zahlen sind gerade nicht abrufbar. Gespielt ist gespielt - sie stehen im
              Protokoll, nicht in diesem Browser.
              <button class="btn btn--quiet" type="button" (click)="player.refresh()">
                Erneut laden
              </button>
            </p>
          } @else {
            <p class="status__why" i18n="@@status.loading">Wird geladen …</p>
          }
        } @else if (!player.unlocked()) {
          <div class="status__head">
            <p class="eyebrow" i18n="@@status.heading">Dein Stand</p>
            <span class="status__count">{{ player.completed() }} / {{ player.unlocksAt() }}</span>
          </div>

          <div
            class="bar"
            role="progressbar"
            [attr.aria-valuenow]="player.completed()"
            aria-valuemin="0"
            [attr.aria-valuemax]="player.unlocksAt()"
          >
            @for (i of cells(); track i) {
              <span class="bar__cell" [class.bar__cell--done]="i < player.completed()"></span>
            }
          </div>

          <!-- The count decides the noun, so it is an ICU plural rather than a ternary: English
               and German agree here, but a language with a dual or a paucal does not, and a
               ternary hard-codes the assumption that two forms are enough. -->
          <p class="status__why" i18n="@@status.locked.short">
            Noch <strong>{{ player.remaining() }}</strong>
            {player.remaining(), plural, =1 {Sitzung} other {Sitzungen}}, dann schaltet die
            Statistik frei.
          </p>
          @if (player.abandoned() > 0) {
            <p class="status__why" i18n="@@status.locked.abandonedShort">
              <strong>{{ player.abandoned() }}</strong>
              {player.abandoned(), plural, =1 {Sitzung} other {Sitzungen}} abgebrochen.
            </p>
          }
        } @else {
          <div class="status__head">
            <p class="eyebrow" i18n="@@status.heading">Dein Stand</p>
            <a class="status__more" routerLink="/statistik" i18n="@@status.allStats">Alle Statistiken</a>
          </div>

          <!-- Four things, in the order somebody actually asks them: what am I, how often did I
               hit, how often should I have, and how surprising is the difference. Everything else
               that used to be here — the deviation, the Wilson floor, the abandoned count, the
               meter — was a second click away behind a "show details" button, which is a button
               that says "the interesting part is elsewhere". It is: it is on the statistics page,
               which is one link away and headed as such. -->
          <div class="status__row">
            <div class="rank">
              <img class="rank__pic" [src]="'rank/rank-' + player.rank().icon + '.svg'" alt="" />
              <span class="rank__box">
                <span class="fig__label" i18n="@@fig.rank">Psi-Rang</span>
                <span class="rank__title">{{ player.rank().title }}</span>
              </span>
            </div>

            <div class="fig">
              <span class="fig__label" i18n="@@fig.hits">Treffer</span>
              <span class="fig__val measured">{{ count(player.reportedHits()) }}</span>
            </div>
            <div class="fig">
              <span class="fig__label" i18n="@@fig.expectedHits">Zufall bringt</span>
              <span class="fig__val measured">{{ expectedHits() }}</span>
            </div>
            <div class="fig">
              <span class="fig__label" i18n="@@fig.luck">Durch Glück</span>
              <span class="fig__val measured">{{ luck() }}</span>
            </div>
          </div>

          <!-- FR-019: the n the last figure stands over, because without it a reader divides one
               number by another and gets a wrong answer. It also says what "durch Glück" counts,
               which a three-word label cannot. -->
          <p class="status__basis" i18n="@@status.basis">
            Über <strong>{{ player.reportedTrials() }}</strong> gewerteten Sitzungen, das sind
            <strong>{{ pct(player.rate()) }} %</strong>. »Durch Glück« heißt: so viele Ratende
            braucht es, bis einer davon genauso weit kommt.
          </p>
        }
      </div>
    }
  `,
  styleUrl: './status-panel.component.scss',
})
export class StatusPanelComponent {
  readonly player = inject(PlayerService);
  readonly session = inject(SessionService);

  /**
   * The locale the bundle was compiled for. Angular sets it from the build, so the German bundle
   * writes `12,5` and the English one `12.5` — which was previously hard-coded to `de-DE` and so
   * printed German decimals on the English domain no matter what.
   */
  private readonly locale = inject(LOCALE_ID);

  /** One cell per trial to the unlock, and the unlock is the server's number (D26, FR-050). */
  readonly cells = computed(() => Array.from({ length: this.player.unlocksAt() }, (_, i) => i));

  /**
   * How many hits pure guessing would have produced over the same run.
   *
   * This cell used to hold the chance *rate*, 12,5 %, beside the account's own rate. Two
   * percentages read as one statistic with a footnote attached; two counts read as a sentence —
   * "21 Treffer, Zufall bringt 15" — and the surplus is a subtraction anybody can do in their
   * head, with no confidence level, no σ and nothing to look up.
   *
   * Taken over `reportedTrials` rather than the live count, because that is the `n` the hits
   * beside it stand over (FR-019). A chance count from the live `n` would put two different runs
   * side by side and invite the reader to subtract them.
   */
  readonly expectedHits = computed(() => this.dec(this.player.reportedTrials() * CHANCE));

  /**
   * How rare this account's result is under pure guessing, as "one in so many".
   *
   * The server publishes it per ten thousand, which is the right unit to compute in and the wrong
   * one to read: "413 von 10 000" makes a reader do the division. Inverted here, and here only —
   * the figure itself still comes from `by_chance_per_10k` and is not recomputed.
   *
   * Zero is not "never". The server rounds, so anything rarer than one in twenty thousand arrives
   * as zero, and the honest rendering of that is a bound rather than a ratio (see
   * `by_chance::per_10_000`).
   */
  readonly luck = computed(() => {
    const per10k = this.player.perTenK();
    if (per10k <= 0) return $localize`:@@fig.luck.rarest:< 1 von 10 000`;
    return $localize`:@@fig.luck.oneIn:1 von ${this.count(Math.round(10_000 / per10k))}:many:`;
  });

  pct(v: number): string {
    return this.dec(v * 100);
  }

  /** One decimal, in the bundle's locale — a chance count is rarely a whole number. */
  dec(v: number): string {
    return v.toLocaleString(this.locale, {
      minimumFractionDigits: 1,
      maximumFractionDigits: 1,
    });
  }

  count(v: number): string {
    return v.toLocaleString(this.locale);
  }
}
