import { Injectable, computed, effect, inject, signal } from '@angular/core';
import { ApiService, MyStats } from './api.service';
import { RankDef, rankFor } from './ranks';
import { SessionService } from './session.service';

/**
 * Where this account stands, from the one place that counts it.
 *
 * Every figure below is read from `GET /api/stats/me` and none is computed here. That is not a
 * matter of taste. The inferential figures — deviation, the Wilson bound, the by-chance count —
 * advance in blocks of completed trials (FR-019, D8): they stand still between block boundaries
 * so that a player cannot watch their z-score wobble and stop on a good one. A browser
 * recomputing them from `completed` and `hits` would produce a *different, moving* number and
 * put it on the same page as the server's, and the site would be arguing with itself about the
 * only figures it asks anyone to believe.
 *
 * `reportedTrials` and `reportedHits` are published for the same reason: they are the `n` those
 * three stand over, and without them a reader divides a deviation by the live trial count and
 * gets a wrong answer.
 */

/**
 * What the progress bar draws before the first answer has arrived.
 *
 * Not a threshold anything is decided by — every decision uses `unlocksAt()`, which is the
 * server's `unlocks_at` and is expected to move (D26, FR-050). This is only how many cells to
 * draw in the moment before the first response, and it is corrected as soon as one lands. It is
 * not exported: a second file reading it would be a second file with an opinion about a number
 * the operator owns.
 */
const ASSUMED_UNLOCK_AT = 10;

@Injectable({ providedIn: 'root' })
export class PlayerService {
  private readonly api = inject(ApiService);
  private readonly session = inject(SessionService);

  private readonly stats = signal<MyStats | null>(null);

  /**
   * The last attempt failed and there is still nothing to show.
   *
   * Paired with `loaded()` rather than with a separate "in flight" flag: the two states a panel
   * has to tell apart are "no figures yet, be patient" and "no figures, and here is a button",
   * and a third signal would only be a second way to ask the same question.
   */
  readonly failed = signal(false);

  /**
   * The holder's own name, whatever review state it is in — the holder is never masked (D25).
   *
   * An empty string counts as absent. An erased account has no name at all (FR-035) and the
   * stored record cannot hold null, so the empty string is how that arrives here; treated as a
   * name it would render the header as a blank gap instead of falling through to the public id.
   */
  readonly name = computed(() => {
    const stored = this.session.account()?.name ?? '';
    return stored === '' ? null : stored;
  });
  readonly publicId = computed(() => this.session.account()?.publicId ?? null);

  readonly loaded = computed(() => this.stats() !== null);

  readonly completed = computed(() => this.stats()?.completed ?? 0);
  readonly abandoned = computed(() => this.stats()?.abandoned ?? 0);
  readonly hits = computed(() => this.stats()?.hits ?? 0);
  readonly unlocksAt = computed(() => this.stats()?.unlocks_at ?? ASSUMED_UNLOCK_AT);

  /**
   * Whether the statistics view is open. Read from the presence of the figures rather than by
   * comparing counts: the server decides this, and it decides it on completed trials alone —
   * never on having hit anything, which would condition the visible population on success (D8).
   */
  readonly unlocked = computed(() => this.stats()?.hit_rate !== undefined);
  readonly remaining = computed(() => Math.max(0, this.unlocksAt() - this.completed()));

  readonly rate = computed(() => this.stats()?.hit_rate ?? 0);
  readonly deviation = computed(() => this.stats()?.deviation ?? 0);
  readonly wilson = computed(() => this.stats()?.wilson_lower ?? 0);
  readonly perTenK = computed(() => this.stats()?.by_chance_per_10k ?? 0);
  readonly distinctDays = computed(() => this.stats()?.distinct_days ?? 0);

  /** The `n` the three inferential figures stand over, and the hits inside it. */
  readonly reportedTrials = computed(() => this.stats()?.reported_trials ?? 0);
  readonly reportedHits = computed(() => this.stats()?.reported_hits ?? 0);

  readonly rank = computed<RankDef>(() => rankFor(this.stats()?.rank));

  constructor() {
    // The figures belong to a token. Signing in loads them; signing out drops them, rather than
    // leaving the previous account's trial count under the page of somebody who is now nobody.
    effect(() => {
      if (this.session.signedIn()) {
        void this.refresh();
        void this.identify();
      } else {
        this.stats.set(null);
        this.failed.set(false);
      }
    });
  }

  /**
   * Asks the server whose account this token opens, unless it is already known.
   *
   * A browser that signed up here has the name in `localStorage` from the moment it was created.
   * One that arrived through an access link has only the token — the two domains are separate
   * origins and the link is a capability, not an identity — so without this the header shows a
   * placeholder for the rest of the session, and the visitor is looking at somebody's account
   * with no way to tell it is theirs.
   *
   * Failures are swallowed. This is a label, not a gate: nothing about playing depends on it, and
   * a red bar over a name is a worse answer than the id that is already on screen.
   */
  private async identify(): Promise<void> {
    if (this.session.account() !== null) return;
    try {
      const own = await this.api.whoami();
      this.session.rememberAccount({ publicId: own.public_id, name: own.name ?? '' });
    } catch {
      // As above: the public id keeps standing in until the next load.
    }
  }

  /**
   * Re-reads the figures.
   *
   * Called after every completed trial rather than incremented locally. A local `completed + 1`
   * is a claim that the answer was recorded, and the case that has to be got right is the one
   * where it was not — an answer that died on the wire must leave the count where the server has
   * it, not where the browser hoped it would be.
   */
  async refresh(): Promise<void> {
    if (!this.session.signedIn()) return;
    try {
      this.stats.set(await this.api.myStats());
      this.failed.set(false);
    } catch {
      // The reason is not surfaced: there is exactly one thing the interface can offer for any of
      // them, which is to try again. The panels say that much and no more.
      this.failed.set(true);
    }
  }
}
