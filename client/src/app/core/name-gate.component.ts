import { Component, computed, inject, output, signal } from '@angular/core';
import { FormsModule } from '@angular/forms';
import { ApiError, ApiService, NetworkError } from './api.service';
import { NAME_MAX, checkDisplayName, normaliseDisplayName } from './display-name';

/**
 * The gate in front of the first trial.
 *
 * It stands where the coordinate normally does, because the coordinate is the start of a session
 * and there is no session to start until the account exists. It says one thing only — the name is
 * public, on the board — because a wall of text in front of a single input field is a wall people
 * stop reading. The rest (moderation, masking, removal) is told where it becomes relevant.
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
      <!-- What the site is, before the first thing it asks for. Visitors kept asking what this
           was even about, and a name field is a bad place to find out: somebody who does not know
           what they are signing up for either leaves or invents a name and never plays, which is
           the row D32 then sweeps. Two short sentences, in the order somebody needs them. -->
      <p class="gate__what" i18n="@@gate.what">
        Hier trainierst du Remote Viewing.<br />
        Du bekommst eine Koordinate, nimmst wahr, was das Ziel sein könnte, und deckst dann acht
        Bilder auf. Eines davon ist das Ziel.
      </p>

      <h1 class="gate__h" i18n="@@gate.heading">Wie sollen wir dich nennen?</h1>

      <p class="gate__lead" i18n="@@gate.lead">Der Name steht auf der öffentlichen Bestenliste.</p>

      <label class="gate__label" for="displayName" i18n="@@gate.label">Anzeigename</label>
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
        <p class="gate__msg" id="nameHint" i18n="@@gate.hint">
          3 bis {{ max }} Zeichen. Buchstaben, Ziffern, Leerzeichen, Bindestrich, Unterstrich.
        </p>
      }

      <button
        class="btn btn--big"
        type="button"
        [disabled]="!check().ok || sending()"
        (click)="submit()"
      >
        @if (sending()) {
          <ng-container i18n="@@gate.creating">Konto wird angelegt …</ng-container>
        } @else {
          <ng-container i18n="@@gate.start">Loslegen</ng-container>
        }
      </button>
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
function refusals(): Record<string, string> {
  return {
    too_short: $localize`:@@refuse.tooShort:Zu kurz.`,
    too_long: $localize`:@@refuse.tooLong:Zu lang.`,
    shapeless: $localize`:@@refuse.shapeless:Der Name muss wie ein Name aussehen: Buchstaben, nicht nur Zeichen und Ziffern.`,
    reserved: $localize`:@@refuse.reserved:Der Name ist für die Seite selbst reserviert.`,
    hate: $localize`:@@refuse.hate:Such dir etwas anderes aus.`,
    vulgar: $localize`:@@refuse.vulgar:Der Name steht auf einer öffentlichen Rangliste. Nicht dieser.`,
    address: $localize`:@@refuse.address:Keine Adressen im Namen.`,
  };
}

/**
 * Why the account was not created — and, above all, *whether it was about the name at all*.
 *
 * The distinction this function exists to keep is between a name the service looked at and
 * refused, and a service that never looked at anything. They used to collapse into one sentence:
 * a `NetworkError` was caught, but a backend that is down behind a proxy that is up produces a
 * perfectly successful `fetch` carrying `502`, which is an `ApiError` with no known code — and
 * fell through to "the name was refused, pick another one". Somebody then sat there rewording a
 * name that was never the problem.
 *
 * So only a `400` is allowed to speak about the name. Everything else says what is actually
 * broken, and says that the name is not it.
 */
function reason(e: unknown): string {
  if (e instanceof NetworkError) {
    return $localize`:@@fail.network:Keine Verbindung zum Server. Es wurde nichts angelegt — dein Name ist in Ordnung, versuch es gleich noch einmal.`;
  }

  if (e instanceof ApiError) {
    // 502/503/504 from the proxy, 500 from the service: it is not answering. A gateway's HTML
    // error page lands here too, because `errorCode` cannot parse it and falls back to the status.
    if (e.status >= 500) {
      return $localize`:@@fail.serverDown:Der Dienst ist gerade nicht erreichbar (Fehler ${e.status}:status:). Das liegt nicht an deinem Namen — versuch es in ein paar Minuten noch einmal.`;
    }

    if (e.status === 400) {
      const spelt = refusals()[e.code];
      if (spelt !== undefined) return spelt;
      return $localize`:@@fail.nameRefused:Der Name wurde abgelehnt. Such dir einen anderen aus.`;
    }

    // A 401, 404, 405 … here means this build and the service disagree about the API. Nothing the
    // visitor types will fix that, so the message must not send them back to the input field.
    return $localize`:@@fail.unexpected:Unerwartete Antwort vom Server (Fehler ${e.status}:status:). Das liegt nicht an deinem Namen.`;
  }

  return $localize`:@@fail.unknown:Das Konto konnte nicht angelegt werden. Das liegt nicht an deinem Namen — versuch es noch einmal.`;
}
