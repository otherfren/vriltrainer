import { Component, computed, inject, output, signal } from '@angular/core';
import { FormsModule } from '@angular/forms';
import { ApiError, ApiService, NetworkError } from './api.service';
import { NAME_MAX, checkDisplayName, normaliseDisplayName } from './display-name';

/**
 * The gate in front of the first trial.
 *
 * It stands where the coordinate normally does, because the coordinate is the start of a session
 * and there is no session to start until the account exists. Everything the name commits you to
 * is said here rather than after the fact: it goes on a public board, next to a trial history
 * that is permanent, and it is the only thing about you that is not opaque.
 *
 * `checkDisplayName` runs here so somebody learns what is wrong while they type. It is **not** the
 * gate — the server applies the same rules on `POST /api/account`, and when the two disagree it
 * is the server's refusal that gets shown, because it is the one that happened.
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
        [disabled]="sending()"
        [attr.aria-invalid]="showError()"
        [attr.aria-describedby]="showError() ? 'nameError' : 'nameHint'"
        [ngModel]="entered()"
        (ngModelChange)="onType($event)"
        (blur)="touched.set(true)"
        (keyup.enter)="submit()"
      />

      @if (refused(); as why) {
        <p class="gate__msg gate__msg--bad" id="nameError">{{ why }}</p>
      } @else if (touched() && !check().ok) {
        <p class="gate__msg gate__msg--bad" id="nameError">{{ check().message }}</p>
      } @else {
        <p class="gate__msg" id="nameHint">
          3 bis {{ max }} Zeichen. Buchstaben, Ziffern, Leerzeichen, Bindestrich, Unterstrich.
        </p>
      }

      <button
        class="btn btn--big"
        type="button"
        [disabled]="!check().ok || sending()"
        (click)="submit()"
      >
        {{ sending() ? 'Konto wird angelegt …' : 'Loslegen' }}
      </button>

      <p class="gate__note">
        Der Name wird vor der Veröffentlichung von einem Menschen freigegeben. Bis dahin steht auf
        öffentlichen Seiten eine Verdeckung — dir selbst zeigen wir ihn immer. Entfernen kannst du
        ihn jederzeit; die Sitzungen bleiben dann unter der Kennung stehen und nachrechenbar.
      </p>
    </div>
  `,
  styleUrl: './name-gate.component.scss',
})
export class NameGateComponent {
  private readonly api = inject(ApiService);

  /** Fired once the account exists, so the screen behind this can start its first trial. */
  readonly ready = output<void>();

  readonly max = NAME_MAX;
  readonly entered = signal('');
  readonly touched = signal(false);
  readonly sending = signal(false);
  /** What the **server** said, which outranks anything the local pre-filter thinks. */
  readonly refused = signal<string | null>(null);

  readonly check = computed(() => checkDisplayName(this.entered()));
  readonly showError = computed(
    () => this.refused() !== null || (this.touched() && !this.check().ok),
  );

  onType(value: string): void {
    this.entered.set(value);
    // A refusal is about one specific string. Once that string changes the refusal is stale, and
    // leaving it on screen makes the next attempt look pre-refused.
    this.refused.set(null);
  }

  async submit(): Promise<void> {
    this.touched.set(true);
    if (!this.check().ok || this.sending()) return;

    this.sending.set(true);
    this.refused.set(null);
    try {
      await this.api.createAccount(normaliseDisplayName(this.entered()));
      this.ready.emit();
    } catch (e) {
      this.refused.set(reason(e));
    } finally {
      this.sending.set(false);
    }
  }
}

/**
 * The server's refusal, in this domain's voice.
 *
 * The API answers with a code and never a sentence, precisely so the sentence can live here and
 * differ between `vriltrainer.de` and `vriltrainer.com` (D10). A code with no entry falls through
 * to a general refusal rather than being shown raw — `shapeless` on screen is neither German nor
 * an explanation.
 */
const REFUSALS: Record<string, string> = {
  too_short: 'Zu kurz.',
  too_long: 'Zu lang.',
  shapeless: 'Der Name muss wie ein Name aussehen: Buchstaben, nicht nur Zeichen und Ziffern.',
  reserved: 'Der Name ist für die Seite selbst reserviert.',
  hate: 'Such dir etwas anderes aus.',
  vulgar: 'Der Name steht auf einer öffentlichen Rangliste. Nicht dieser.',
  address: 'Keine Adressen im Namen.',
};

function reason(e: unknown): string {
  if (e instanceof NetworkError) {
    return 'Keine Verbindung zum Server. Es wurde nichts angelegt — versuch es noch einmal.';
  }
  if (e instanceof ApiError) {
    if (e.status === 429) {
      return 'Von dieser Verbindung wurden gerade viele Konten angelegt. Versuch es später noch einmal.';
    }
    const spelt = REFUSALS[e.code];
    if (spelt !== undefined) return spelt;
  }
  return 'Der Name wurde abgelehnt. Such dir einen anderen aus.';
}
