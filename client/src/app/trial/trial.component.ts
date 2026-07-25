import { Component, computed, inject, signal } from '@angular/core';
import {
  ApiError,
  ApiService,
  Answered,
  NetworkError,
  Revealed,
  TrialStart,
} from '../core/api.service';
import { PlayerService } from '../core/player.service';
import { SessionService } from '../core/session.service';
import { NameGateComponent } from '../core/name-gate.component';
import { categoryLabel, imageSrc } from '../core/pool';
import { PoolManifest } from '../verify/manifest';
import { Proof, verifyTrial } from '../verify/proof';

/**
 * Sealed → revealed → answered, plus the two states a demo never had: waiting, and failed.
 *
 * `answering` is separate from `answered` for one reason, and it is why the whole state machine
 * is written out rather than inferred from which fields happen to be set. A trial whose answer is
 * in flight is **not** a completed trial. If the request dies on the wire the server may or may
 * not have recorded it; what is certain is that this browser does not know, and the one thing the
 * screen must never do is show a verdict — or a trial count — that nobody wrote down.
 */
type Stage = 'starting' | 'sealed' | 'revealing' | 'revealed' | 'answering' | 'answered';

interface Slot {
  id: string;
  src: string;
  /** From the manifest, shown only once the answer is in. Null if the manifest is not here. */
  category: string | null;
  dealt: boolean;
}

/** Something the user has to be told, and whether the trial survived it. */
interface Notice {
  text: string;
  /** `true` when the trial is over and only a new one can follow. */
  fatal: boolean;
}

@Component({
  selector: 'app-trial',
  standalone: true,
  imports: [NameGateComponent],
  templateUrl: './trial.component.html',
  styleUrl: './trial.component.scss',
})
export class TrialComponent {
  private readonly api = inject(ApiService);
  readonly player = inject(PlayerService);
  readonly session = inject(SessionService);

  readonly stage = signal<Stage>('starting');
  readonly trial = signal<TrialStart | null>(null);
  readonly slots = signal<Slot[]>([]);
  readonly chosen = signal<string | null>(null);
  readonly answered = signal<Answered | null>(null);
  readonly notice = signal<Notice | null>(null);
  readonly tooFast = signal(false);

  readonly proof = signal<Proof | null>(null);
  readonly proofFailed = signal(false);
  readonly proofOpen = signal(false);

  /** The trial's current token. It is replaced at the reveal; the previous one is spent. */
  private token: string | null = null;
  private revealed: Revealed | null = null;
  private manifest: PoolManifest | null = null;

  readonly hit = computed(() => this.answered()?.hit ?? false);
  readonly target = computed(() => this.answered()?.target ?? null);

  constructor() {
    // Nothing starts before there is an account. A trial without a token is a guaranteed 401, and
    // what belongs on the screen in that case is the name gate, not an error.
    if (this.session.signedIn()) void this.next();
  }

  /**
   * Starts a trial.
   *
   * The manifest is fetched alongside and not awaited: the proof needs it, the images do not, and
   * a slow pool download must not hold up the coordinate. A failure here is retried when the
   * proof is built, which is the first moment its absence costs anything.
   */
  async next(): Promise<void> {
    this.stage.set('starting');
    this.slots.set([]);
    this.chosen.set(null);
    this.answered.set(null);
    this.notice.set(null);
    this.proof.set(null);
    this.proofFailed.set(false);
    this.proofOpen.set(false);
    this.token = null;
    this.revealed = null;

    try {
      const started = await this.api.startTrial();
      this.trial.set(started);
      this.token = started.token;
      this.stage.set('sealed');
      void this.loadManifest(started.poolVersion).catch(() => undefined);
    } catch (e) {
      this.trial.set(null);
      this.fail(e, {
        // D17: an account may hold only so many uncompleted trials at once. "Too many requests"
        // would be true and useless; this says what to do about it.
        429: 'Du hast zu viele offene Sitzungen. Beende oder warte eine ab, dann geht es weiter.',
      });
    }
  }

  /** Contributes this browser's randomness and deals the eight (D1, D3). */
  async reveal(): Promise<void> {
    if (this.token === null || this.stage() !== 'sealed') return;
    this.stage.set('revealing');
    this.notice.set(null);

    let revealed: Revealed;
    try {
      revealed = await this.api.reveal(this.token);
    } catch (e) {
      // The commit is on disk and the trial is still open, so the honest state to return to is
      // the one we were in. A trial abandoned here is published as abandoned (FR-021), which is
      // precisely why the screen must not quietly move on.
      this.stage.set('sealed');
      this.fail(
        e,
        {
          410: 'Die Gültigkeit dieser Sitzung ist abgelaufen. Sie zählt als abgebrochen — fang eine neue an.',
          409: 'Diese Sitzung ist bereits beantwortet.',
        },
        false,
      );
      return;
    }

    this.revealed = revealed;
    this.token = revealed.token;
    this.slots.set(
      revealed.images.map((id) => ({ id, src: imageSrc(id), category: null, dealt: false })),
    );
    this.stage.set('revealed');
    this.deal();
  }

  /**
   * Submits a choice.
   *
   * The three-second rule is the server's — checked there before the chosen image is examined at
   * all, or the refusal itself would answer "was that the target?" for anyone willing to guess
   * fast and read a status code (FR-039, SC-016). This client deliberately does not pre-empt it:
   * a browser clock is not the clock the rule is written against, and a client-side gate would
   * only hide the real one from the person it applies to.
   */
  async choose(id: string): Promise<void> {
    if (this.stage() !== 'revealed' || this.token === null) return;
    this.chosen.set(id);
    this.stage.set('answering');
    this.tooFast.set(false);
    this.notice.set(null);

    let answered: Answered;
    try {
      answered = await this.api.answer(this.token, id);
    } catch (e) {
      this.chosen.set(null);
      this.stage.set('revealed');

      if (e instanceof ApiError && e.status === 425) {
        // Nothing was written and nothing was looked at. The trial is untouched and answerable.
        this.tooFast.set(true);
        setTimeout(() => this.tooFast.set(false), 2600);
        return;
      }
      if (e instanceof NetworkError) {
        // The case this whole state machine exists for. The request may have been executed; this
        // browser cannot tell. So: no verdict, no local count, and the figures are re-read from
        // the server, which is the only party that knows what happened.
        this.notice.set({
          text:
            'Die Antwort ist nicht angekommen. Ob sie gewertet wurde, weiß dieser Browser nicht — ' +
            'dein Stand unten steht so, wie der Server ihn zählt.',
          fatal: false,
        });
        void this.player.refresh();
        return;
      }
      this.fail(e, {
        409: 'Diese Sitzung ist bereits beantwortet. Eine Antwort pro Sitzung, und sie steht schon im Protokoll.',
        410: 'Die Gültigkeit dieser Sitzung ist abgelaufen. Sie zählt als abgebrochen — fang eine neue an.',
      });
      return;
    }

    this.answered.set(answered);
    this.stage.set('answered');
    // From the server, never from a local increment: this is the moment the trial became
    // completed, and the server is the only party that knows it did.
    void this.player.refresh();
    void this.buildProof(answered);
  }

  /** The recomputation, from what the server just handed over and nothing else. */
  private async buildProof(answered: Answered): Promise<void> {
    const trial = this.trial();
    const revealed = this.revealed;
    if (trial === null || revealed === null) return;

    this.proofFailed.set(false);
    try {
      const manifest = this.manifest ?? (await this.loadManifest(trial.poolVersion));
      this.proof.set(
        await verifyTrial(
          {
            coordinate: trial.coordinate,
            commitment: trial.commitment,
            poolVersion: trial.poolVersion,
            poolManifestHash: trial.poolManifestHash,
          },
          { sent: revealed.sentSClient, images: revealed.images },
          answered,
          manifest,
        ),
      );
      this.label(manifest);
    } catch {
      // Without the manifest there is no derivation to show, and saying so is the only honest
      // option: an empty proof panel reads as "checked, nothing to report".
      this.proofFailed.set(true);
    }
  }

  /** Retries the recomputation after the manifest failed to arrive. */
  retryProof(): void {
    const answered = this.answered();
    if (answered !== null) void this.buildProof(answered);
  }

  private async loadManifest(version: number): Promise<PoolManifest> {
    const manifest = await this.api.manifest(version);
    this.manifest = manifest;
    return manifest;
  }

  /** Category names, attached only once the verdict is in. */
  private label(manifest: PoolManifest): void {
    const categories = new Map(manifest.images.map((e) => [e.id, e.category]));
    this.slots.update((slots) =>
      slots.map((s) => {
        const category = categories.get(s.id);
        return { ...s, category: category === undefined ? null : categoryLabel(category) };
      }),
    );
  }

  /**
   * One orchestrated moment rather than scattered effects: the eight are dealt in order, each
   * landing with a small overshoot. Nothing else on the page moves by itself.
   */
  private deal(): void {
    const reduce = matchMedia('(prefers-reduced-motion: reduce)').matches;
    this.slots().forEach((_, i) => {
      setTimeout(
        () => this.slots.update((s) => s.map((x, j) => (j === i ? { ...x, dealt: true } : x))),
        reduce ? 0 : 70 * i,
      );
    });
  }

  /**
   * Turns a thrown failure into one sentence and a state.
   *
   * `spelled` carries the statuses this call site can say something specific about. Everything
   * else gets the same two sentences, because everything else has the same remedy.
   */
  private fail(e: unknown, spelled: Record<number, string>, fatal = true): void {
    if (e instanceof ApiError && e.status === 401) {
      // Deliberately *not* a sign-out. The token in this browser may be the only copy in
      // existence (D9, FR-005), and discarding it on one refusal would destroy the account over
      // what could as easily be a server started against the wrong database. So: say what
      // happened, and point at the one action that still helps.
      this.notice.set({
        text:
          'Der Server kennt dieses Login nicht. Sichere den Link über „Login“ oben rechts, bevor ' +
          'du etwas anderes tust — es gibt keine Wiederherstellung.',
        fatal: false,
      });
      return;
    }
    const spelt = e instanceof ApiError ? spelled[e.status] : undefined;
    const text =
      spelt ??
      (e instanceof NetworkError
        ? 'Keine Verbindung zum Server. Versuch es gleich noch einmal.'
        : 'Der Server hat die Anfrage abgelehnt. Versuch es noch einmal.');
    this.notice.set({ text, fatal });
  }

  /** How a slot is drawn once the verdict is in. */
  slotState(id: string): string {
    if (this.stage() !== 'answered') return '';
    if (id === this.target()) return 'target';
    if (id === this.chosen()) return 'wrong';
    return 'dim';
  }
}
