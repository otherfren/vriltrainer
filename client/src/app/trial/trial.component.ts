import { Component, inject, signal } from '@angular/core';
import { ApiService, TrialStart } from '../core/api.service';

type Phase = 'sealed' | 'revealed' | 'answered';

interface Slot {
  index: number;
  src: string;
  developed: boolean;
}

@Component({
  selector: 'app-trial',
  standalone: true,
  templateUrl: './trial.component.html',
  styleUrl: './trial.component.scss',
})
export class TrialComponent {
  private api = inject(ApiService);

  phase = signal<Phase>('sealed');
  trial = signal<TrialStart>(this.api.newTrial());
  slots = signal<Slot[]>([]);
  chosen = signal<number | null>(null);
  target = signal<number | null>(null);
  proofOpen = signal(false);
  tooFast = signal(false);

  /** Trials completed in this session, purely so the interface can show progress. */
  completed = signal(7);
  readonly statsUnlockAt = 10;

  private revealedAt = 0;
  private readonly minimumViewingSeconds = 3;

  reveal(): void {
    const slots: Slot[] = Array.from({ length: 8 }, (_, i) => ({
      index: i,
      src: `demo/target-${i + 1}.svg`,
      developed: false,
    }));
    this.slots.set(slots);
    this.phase.set('revealed');
    this.revealedAt = Date.now();

    // One orchestrated moment rather than scattered effects: the frames develop in order, the
    // way a print comes up in a tray.
    const reduce = matchMedia('(prefers-reduced-motion: reduce)').matches;
    slots.forEach((_, i) => {
      setTimeout(
        () => this.slots.update((s) => s.map((x, j) => (j === i ? { ...x, developed: true } : x))),
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
    this.target.set(Math.floor(Math.random() * 8));
    this.phase.set('answered');
    this.completed.update((n) => n + 1);
  }

  get isHit(): boolean {
    return this.chosen() !== null && this.chosen() === this.target();
  }

  next(): void {
    this.trial.set(this.api.newTrial());
    this.slots.set([]);
    this.chosen.set(null);
    this.target.set(null);
    this.proofOpen.set(false);
    this.phase.set('sealed');
  }

  slotState(i: number): string {
    if (this.phase() !== 'answered') return this.chosen() === i ? 'picked' : '';
    if (i === this.target()) return 'target';
    if (i === this.chosen()) return 'wrong';
    return 'dim';
  }
}
