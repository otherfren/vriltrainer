import { Component, LOCALE_ID, computed, inject, signal } from '@angular/core';
import { RouterLink } from '@angular/router';
import { VrilMeterComponent } from './vril-meter.component';
import { PlayerService } from './player.service';
import { SessionService } from './session.service';

/**
 * Where you stand, under every page.
 *
 * It used to be the aggregate meter, which said the same thing on every page forever and was
 * about everyone rather than you. This says one useful thing at a time: before the unlock, how
 * far you are from a number that means anything; after it, the number — with the statistics that
 * make it interpretable one click away rather than in your face.
 *
 * Every figure comes from `GET /api/stats/me`. Two states exist only because it does: the first
 * load, where there is nothing to draw yet, and the failure, where there is nothing to draw and a
 * reason. Drawing zeroes in either would be a claim about the account that nobody made.
 */
@Component({
  selector: 'app-status-panel',
  standalone: true,
  imports: [RouterLink, VrilMeterComponent],
  template: `
    @if (session.signedIn()) {
      <div class="status panel">
        @if (!player.loaded()) {
          <div class="status__head">
            <p class="eyebrow" i18n="@@status.heading">Dein Stand</p>
          </div>
          @if (player.failed()) {
            <p class="status__why" i18n="@@status.unavailable">
              Die Zahlen sind gerade nicht abrufbar. Gespielt ist gespielt — sie stehen im
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

          <div class="status__row">
            <div class="fig">
              <span class="fig__label" i18n="@@fig.trials">Sitzungen</span>
              <span class="fig__val measured">{{ player.completed() }}</span>
            </div>
            <div class="fig">
              <span class="fig__label" i18n="@@fig.hits">Treffer</span>
              <span class="fig__val measured">{{ player.hits() }}</span>
            </div>
            <div class="fig">
              <span class="fig__label" i18n="@@fig.rate">Quote</span>
              <span class="fig__val measured">{{ pct(player.rate()) }} %</span>
            </div>

            <div class="rank">
              <img class="rank__pic" [src]="'rank/rank-' + player.rank().icon + '.svg'" alt="" />
              <span class="rank__box">
                <span class="fig__label" i18n="@@fig.rank">Psi-Rang</span>
                <span class="rank__title">{{ player.rank().title }}</span>
              </span>
            </div>
          </div>

          <button class="btn btn--quiet status__toggle" type="button" (click)="open.set(!open())">
            @if (open()) {
              <ng-container i18n="@@status.details.hide">Details schließen</ng-container>
            } @else {
              <ng-container i18n="@@status.details.show">Details anzeigen</ng-container>
            }
          </button>

          @if (open()) {
            <div class="detail">
              <div class="detail__grid">
                <div class="fig">
                  <span class="fig__label" i18n="@@fig.deviation">Abweichung</span>
                  <span class="fig__val measured">{{ signed(player.deviation()) }} σ</span>
                </div>
                <div class="fig">
                  <span class="fig__label" i18n="@@fig.wilson">Mindestens</span>
                  <span class="fig__val measured">{{ pct(player.wilson()) }} %</span>
                </div>
                <div class="fig">
                  <span class="fig__label" i18n="@@fig.abandoned">Abgebrochen</span>
                  <span class="fig__val measured">{{ player.abandoned() }}</span>
                </div>
                <div class="fig">
                  <span class="fig__label" i18n="@@fig.chance">Zufallsrate</span>
                  <span class="fig__val measured">{{ chanceRate }} %</span>
                </div>
              </div>

              <app-vril-meter
                [deviation]="player.deviation()"
                [mine]="true"
                i18n-label="@@meter.you.label"
                label="Du"
                i18n-needleLabel="@@meter.you.needle"
                needleLabel="du"
                [reading]="reading()"
              />

              <!-- FR-019: the n these figures stand over, because without it a reader divides a
                   deviation by the live trial count and gets a wrong answer. A caption, not an
                   essay — the reasoning behind block-wise reporting is not what somebody opened
                   this panel to read. -->
              <p class="detail__note" i18n="@@status.detail.basis">
                Über <strong>{{ player.reportedTrials() }}</strong> gewerteten Sitzungen,
                <strong>{{ player.reportedHits() }}</strong> Treffer.
              </p>
            </div>
          }
        }
      </div>
    }
  `,
  styleUrl: './status-panel.component.scss',
})
export class StatusPanelComponent {
  readonly player = inject(PlayerService);
  readonly session = inject(SessionService);
  readonly open = signal(false);

  /**
   * The locale the bundle was compiled for. Angular sets it from the build, so the German bundle
   * writes `12,5` and the English one `12.5` — which was previously hard-coded to `de-DE` and so
   * printed German decimals on the English domain no matter what.
   */
  private readonly locale = inject(LOCALE_ID);

  /** One cell per trial to the unlock, and the unlock is the server's number (D26, FR-050). */
  readonly cells = computed(() => Array.from({ length: this.player.unlocksAt() }, (_, i) => i));

  /** Chance is 1 in 8 by construction (D3); written as a figure so it follows the locale. */
  readonly chanceRate = this.pct(0.125);

  readonly reading = computed(() =>
    $localize`:@@meter.you.reading:${this.signed(this.player.deviation())}:deviation: σ · Mindestquote ${this.pct(this.player.wilson())}:wilson: %`,
  );

  pct(v: number): string {
    return (v * 100).toLocaleString(this.locale, {
      minimumFractionDigits: 1,
      maximumFractionDigits: 1,
    });
  }

  signed(v: number): string {
    const s = v.toLocaleString(this.locale, {
      minimumFractionDigits: 2,
      maximumFractionDigits: 2,
    });
    return v >= 0 ? `+${s}` : s;
  }
}
