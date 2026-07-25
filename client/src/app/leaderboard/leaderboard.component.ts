import { Component, computed, inject, signal } from '@angular/core';
import { RouterLink } from '@angular/router';
import { ApiService, Board, BoardEntry } from '../core/api.service';
import { MASKED_NAME } from '../core/display-name';
import { RankDef, ladder, rankFor, rankIcon } from '../core/ranks';

/**
 * The board, straight from `GET /api/leaderboard`.
 *
 * Two things it deliberately does not do.
 *
 * It does not sort, re-place or re-band anything. The server numbers the places in the same order
 * it lists the rows in, and a second ordering in the browser is a second chance for the two to
 * disagree about who is first — with nothing about the result looking like a bug until somebody
 * counts.
 *
 * It does not know how many rows there are. The API answers a window and says nothing about the
 * total, so the pager is previous/next rather than numbered pages: a full page means there is
 * probably more, and that is the whole of what can honestly be claimed.
 */
@Component({
  selector: 'app-leaderboard',
  standalone: true,
  imports: [RouterLink],
  templateUrl: './leaderboard.component.html',
  styleUrl: './leaderboard.component.scss',
})
export class LeaderboardComponent {
  private readonly api = inject(ApiService);

  readonly pageSize = 20;
  readonly offset = signal(0);
  readonly board = signal<Board | null>(null);
  readonly loading = signal(true);
  readonly failed = signal(false);

  readonly entries = computed<BoardEntry[]>(() => this.board()?.entries ?? []);
  readonly eligible = computed(() => this.board()?.eligible_accounts ?? 0);

  /** A full page is the only evidence there is that another one exists. */
  readonly hasNext = computed(() => this.entries().length === this.pageSize);
  readonly hasPrevious = computed(() => this.offset() > 0);

  readonly firstShown = computed(() => this.offset() + 1);
  readonly lastShown = computed(() => this.offset() + this.entries().length);

  /**
   * The nearest band that does not exist yet, and the population it needs.
   *
   * FR-042: the board reports `eligible_accounts` whether or not any band is active precisely so
   * it can say how far off the next one is. A band appears once `share × eligible >= 1`, with no
   * rounding, so on a young site this line is the ladder's only visible progress.
   */
  readonly nextBand = computed(() => {
    const t = this.board()?.thresholds;
    if (t === undefined) return null;
    const missing = ladder(t.bands)
      .filter((r) => !r.middle && r.unlocksAt > this.eligible())
      .sort((a, b) => a.unlocksAt - b.unlocksAt);
    return missing[0] ?? null;
  });

  readonly eligibilityTrials = computed(() => this.board()?.thresholds.eligibility_trials ?? 0);
  readonly eligibilityDays = computed(() => this.board()?.thresholds.eligibility_days ?? 0);
  readonly updatedAt = computed(() => this.board()?.ranks_updated_at ?? null);

  constructor() {
    void this.load(0);
  }

  async load(offset: number): Promise<void> {
    this.loading.set(true);
    this.failed.set(false);
    try {
      const board = await this.api.leaderboard(Math.max(0, offset), this.pageSize);
      this.board.set(board);
      // From the response, not from the argument: the server clamps, and the pager must count in
      // the same units the rows were actually taken from.
      this.offset.set(board.offset);
    } catch {
      this.failed.set(true);
    } finally {
      this.loading.set(false);
    }
  }

  step(by: number): void {
    void this.load(this.offset() + by * this.pageSize);
    scrollTo({ top: 0, behavior: 'smooth' });
  }

  rank(entry: BoardEntry): RankDef {
    return rankFor(entry.band);
  }

  icon(entry: BoardEntry): string {
    return rankIcon(this.rank(entry).slug);
  }

  /** A row whose name has not been cleared for publication yet (D25, FR-047). */
  masked(entry: BoardEntry): boolean {
    return entry.name === MASKED_NAME;
  }

  de(n: number, digits = 1): string {
    return n.toLocaleString('de-DE', {
      minimumFractionDigits: digits,
      maximumFractionDigits: digits,
    });
  }

  /** The board reports rates as fractions; the page has always shown them as percentages. */
  pct(n: number, digits = 1): string {
    return this.de(n * 100, digits);
  }

  signed(n: number): string {
    return (n >= 0 ? '+' : '') + this.de(n, 1);
  }

  when(iso: string): string {
    const at = new Date(iso);
    return Number.isNaN(at.getTime()) ? iso : at.toLocaleString('de-DE');
  }
}
