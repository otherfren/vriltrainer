import { Component, inject, signal } from '@angular/core';
import { ApiService, Resolution, TrialStart } from '../core/api.service';
import { PlayerService, STATS_UNLOCK_AT } from '../core/player.service';
import { NameGateComponent } from '../core/name-gate.component';

type Phase = 'sealed' | 'revealed' | 'answered';

interface Slot {
  index: number;
  src: string;
  category: string;
  dealt: boolean;
}

@Component({
  selector: 'app-trial',
  standalone: true,
  imports: [NameGateComponent],
  templateUrl: './trial.component.html',
  styleUrl: './trial.component.scss',
})
export class TrialComponent {
  private api = inject(ApiService);
  readonly player = inject(PlayerService);

  phase = signal<Phase>('sealed');
  trial = signal<TrialStart | null>(null);
  slots = signal<Slot[]>([]);
  chosen = signal<number | null>(null);
  resolution = signal<Resolution | null>(null);
  proofOpen = signal(false);
  tooFast = signal(false);

  readonly statsUnlockAt = STATS_UNLOCK_AT;
  readonly poolNote = this.api.poolNote;

  private revealedAt = 0;
  private readonly minimumViewingSeconds = 3;

  constructor() {
    void this.next();
  }

  async reveal(): Promise<void> {
    const t = this.trial();
    if (!t) return;

    // The client's half of the seed is contributed here, so the eight and the target below are
    // a real run of the real derivation rather than a picture of one.
    const resolved = await this.api.reveal(t.trialId);
    this.resolution.set(resolved);

    const slots: Slot[] = resolved.candidates.map((c, i) => ({
      index: i,
      src: c.src,
      category: c.category,
      dealt: false,
    }));
    this.slots.set(slots);
    this.phase.set('revealed');
    this.revealedAt = Date.now();

    // One orchestrated moment rather than scattered effects: the eight are dealt in order, each
    // landing with a small overshoot. Nothing else on the page moves by itself.
    const reduce = matchMedia('(prefers-reduced-motion: reduce)').matches;
    slots.forEach((_, i) => {
      setTimeout(
        () => this.slots.update((s) => s.map((x, j) => (j === i ? { ...x, dealt: true } : x))),
        reduce ? 0 : 70 * i,
      );
    });
  }

  choose(i: number): void {
    if (this.phase() !== 'revealed') return;

    // The minimum viewing time is checked before the choice is looked at. A refusal that depended
    // on which image was picked would be an oracle for the target (FR-039).
    const elapsed = (Date.now() - this.revealedAt) / 1000;
    if (elapsed < this.minimumViewingSeconds) {
      this.tooFast.set(true);
      setTimeout(() => this.tooFast.set(false), 2600);
      return;
    }

    this.chosen.set(i);
    this.phase.set('answered');
    this.player.record(this.isHit);
  }

  get target(): number | null {
    return this.resolution()?.targetSlot ?? null;
  }

  get isHit(): boolean {
    return this.chosen() !== null && this.chosen() === this.target;
  }

  async next(): Promise<void> {
    this.slots.set([]);
    this.chosen.set(null);
    this.resolution.set(null);
    this.proofOpen.set(false);
    this.phase.set('sealed');
    this.trial.set(await this.api.newTrial());
  }

  slotState(i: number): string {
    if (this.phase() !== 'answered') return '';
    if (i === this.target) return 'target';
    if (i === this.chosen()) return 'wrong';
    return 'dim';
  }
}
