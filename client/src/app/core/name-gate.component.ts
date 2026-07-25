import { Component, computed, inject, signal } from '@angular/core';
import { FormsModule } from '@angular/forms';
import { PlayerService } from './player.service';
import { NAME_MAX, checkDisplayName, normaliseDisplayName } from './display-name';

/**
 * The gate in front of the first trial.
 *
 * It stands where the coordinate normally does, because the coordinate is the start of a
 * session and there is no session to start until the account has a name. Everything the name
 * commits you to is said here rather than after the fact: it goes on a public board, next to a
 * trial history that is permanent, and it is the only thing about you that is not opaque.
 */
@Component({
  selector: 'app-name-gate',
  standalone: true,
  imports: [FormsModule],
  template: `
    <div class="gate panel">
      <p class="eyebrow">Bevor es losgeht</p>
      <h1 class="gate__h">Wie sollen wir dich nennen?</h1>

      <p class="gate__lead">
        Der Name steht auf der öffentlichen Rangliste und neben jeder einzelnen Sitzung im
        herunterladbaren Protokoll. Er ist das Einzige an dir, das nicht anonym ist — alles
        andere läuft unter einer zufälligen Kennung.
      </p>

      <label class="gate__label" for="displayName">Anzeigename</label>
      <input
        id="displayName"
        class="gate__input"
        type="text"
        autocomplete="nickname"
        spellcheck="false"
        [attr.maxlength]="max"
        [attr.aria-invalid]="touched() && !check().ok"
        [attr.aria-describedby]="touched() && !check().ok ? 'nameError' : 'nameHint'"
        [ngModel]="entered()"
        (ngModelChange)="entered.set($event)"
        (blur)="touched.set(true)"
        (keyup.enter)="submit()"
      />

      @if (touched() && !check().ok) {
        <p class="gate__msg gate__msg--bad" id="nameError">{{ check().message }}</p>
      } @else {
        <p class="gate__msg" id="nameHint">
          3 bis {{ max }} Zeichen. Buchstaben, Ziffern, Leerzeichen, Bindestrich, Unterstrich.
        </p>
      }

      <button class="btn btn--big" type="button" [disabled]="!check().ok" (click)="submit()">
        Loslegen
      </button>

      <p class="gate__note">
        Du kannst den Namen später jederzeit entfernen. Die Sitzungen bleiben dann unter der
        Kennung stehen und nachrechenbar — löschen ließe sich sonst die Beweisbarkeit gleich mit.
      </p>
    </div>
  `,
  styleUrl: './name-gate.component.scss',
})
export class NameGateComponent {
  private player = inject(PlayerService);

  readonly max = NAME_MAX;
  readonly entered = signal('');
  readonly touched = signal(false);
  readonly check = computed(() => checkDisplayName(this.entered()));

  submit(): void {
    this.touched.set(true);
    if (!this.check().ok) return;
    this.player.name.set(normaliseDisplayName(this.entered()));
  }
}
