import { Component, computed, inject, signal } from '@angular/core';
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
            <p class="eyebrow">Dein Stand</p>
          </div>
          @if (player.failed()) {
            <p class="status__why">
              Die Zahlen sind gerade nicht abrufbar. Gespielt ist gespielt — sie stehen im
              Protokoll, nicht in diesem Browser.
              <button class="btn btn--quiet" type="button" (click)="player.refresh()">
                Erneut laden
              </button>
            </p>
          } @else {
            <p class="status__why">Wird geladen …</p>
          }
        } @else if (!player.unlocked()) {
          <div class="status__head">
            <p class="eyebrow">Dein Stand</p>
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

          <p class="status__why">
            Noch <strong>{{ player.remaining() }}</strong>
            {{ player.remaining() === 1 ? 'Sitzung' : 'Sitzungen' }}, dann schaltet die Statistik
            frei. Vorher ist eine Trefferquote keine Aussage: bei so wenigen Versuchen sieht reines
            Raten regelmäßig nach einer Begabung aus.
            @if (player.abandoned() > 0) {
              <br />
              <strong>{{ player.abandoned() }}</strong>
              {{ player.abandoned() === 1 ? 'Sitzung' : 'Sitzungen' }} abgebrochen — auch das steht
              im Protokoll, von der ersten an.
            }
          </p>
        } @else {
          <div class="status__head">
            <p class="eyebrow">Dein Stand</p>
            <a class="status__more" routerLink="/statistik">Alle Statistiken</a>
          </div>

          <div class="status__row">
            <div class="fig">
              <span class="fig__label">Sitzungen</span>
              <span class="fig__val measured">{{ player.completed() }}</span>
            </div>
            <div class="fig">
              <span class="fig__label">Treffer</span>
              <span class="fig__val measured">{{ player.hits() }}</span>
            </div>
            <div class="fig">
              <span class="fig__label">Quote</span>
              <span class="fig__val measured">{{ pct(player.rate()) }} %</span>
            </div>

            <div class="rank">
              <img class="rank__pic" [src]="'rank/rank-' + player.rank().icon + '.svg'" alt="" />
              <span class="rank__box">
                <span class="fig__label">Psi-Rang</span>
                <span class="rank__title">{{ player.rank().title }}</span>
              </span>
            </div>
          </div>

          <button class="btn btn--quiet status__toggle" type="button" (click)="open.set(!open())">
            {{ open() ? 'Details schließen' : 'Details anzeigen' }}
          </button>

          @if (open()) {
            <div class="detail">
              <div class="detail__grid">
                <div class="fig">
                  <span class="fig__label">Abweichung</span>
                  <span class="fig__val measured">{{ signed(player.deviation()) }} σ</span>
                </div>
                <div class="fig">
                  <span class="fig__label">Gesicherte Mindestquote</span>
                  <span class="fig__val measured">{{ pct(player.wilson()) }} %</span>
                </div>
                <div class="fig">
                  <span class="fig__label">Abgebrochen</span>
                  <span class="fig__val measured">{{ player.abandoned() }}</span>
                </div>
                <div class="fig">
                  <span class="fig__label">Zufallsrate</span>
                  <span class="fig__val measured">12,5 %</span>
                </div>
              </div>

              <app-vril-meter
                [deviation]="player.deviation()"
                [mine]="true"
                label="Du"
                needleLabel="du"
                [reading]="
                  signed(player.deviation()) + ' σ · Mindestquote ' + pct(player.wilson()) + ' %'
                "
              />

              <p class="detail__note">
                Die Abweichung ist die auffälligere Zahl, die Mindestquote die belastbarere: sie
                wiegt mit, wie oft du gespielt hast. Beide stehen über
                <strong>{{ player.reportedTrials() }}</strong> gewerteten Sitzungen mit
                <strong>{{ player.reportedHits() }}</strong> Treffern und rücken erst am nächsten
                Blockende weiter — sonst könnte man aufhören, sobald die Zahl gefällt.
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

  /** One cell per trial to the unlock, and the unlock is the server's number (D26, FR-050). */
  readonly cells = computed(() => Array.from({ length: this.player.unlocksAt() }, (_, i) => i));

  pct(v: number): string {
    return (v * 100).toLocaleString('de-DE', { minimumFractionDigits: 1, maximumFractionDigits: 1 });
  }

  signed(v: number): string {
    const s = v.toLocaleString('de-DE', { minimumFractionDigits: 2, maximumFractionDigits: 2 });
    return v >= 0 ? `+${s}` : s;
  }
}
