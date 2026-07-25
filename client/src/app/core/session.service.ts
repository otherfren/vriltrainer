import { Injectable, computed, signal } from '@angular/core';

/**
 * The access token, and how it gets into this browser.
 *
 * D9 puts the token in the URL **fragment** — `https://vriltrainer.de/#t=…` — because a fragment
 * is never transmitted: it is absent from the request, from the access log and from `Referer`.
 * That is the entire security argument for a capability URL, and it survives only if this file
 * is the one place the fragment is touched. Nothing here ever puts the token in a path, a query
 * string or a log line.
 *
 * The address bar is cleared with `history.replaceState` on arrival. The bookmark the user
 * followed keeps its fragment; the bar they are about to screenshot does not (D9, D21, FR-006).
 */

/** Storage keys. Namespaced, because both domains may share a browser profile. */
const TOKEN_KEY = 'vriltrainer.access_token';
const ACCOUNT_KEY = 'vriltrainer.account';

/** What the holder knows about their own account. */
export interface Account {
  publicId: string;
  /**
   * The holder's own name, whatever review state it is in. The holder is not a stranger and
   * always sees it (D25); the mask is for everybody else and is applied by the server.
   */
  name: string;
}

@Injectable({ providedIn: 'root' })
export class SessionService {
  private readonly stored = new Store();

  readonly token = signal<string | null>(null);
  readonly account = signal<Account | null>(null);

  /**
   * Whether there is an account to play as — keyed on the **token**, never on the name.
   *
   * A browser that arrived through an access link holds a token and no name, and gating the
   * trial screen on the name would show it the "choose a name" form, which creates a *second*
   * account and silently orphans the first (FR-005: there is no recovery).
   */
  readonly signedIn = computed(() => this.token() !== null);

  /**
   * The link that is the account. Built from the origin actually being served, so the copy
   * button on `vriltrainer.com` never hands somebody a `.de` link with a token the `.de`
   * process would refuse — the two domains are two processes over one database (D24), but a
   * token is redeemed against the origin it was issued for.
   */
  readonly accessLink = computed(() => {
    const t = this.token();
    return t === null ? null : `${location.origin}/#t=${t}`;
  });

  constructor() {
    const fromLink = this.takeFragment();
    if (fromLink !== null) {
      this.adopt(fromLink);
      return;
    }
    this.token.set(this.stored.get(TOKEN_KEY));
    this.account.set(parseAccount(this.stored.get(ACCOUNT_KEY)));
  }

  /** After `POST /api/account`: the one moment the token is ever handed out (FR-002, D9). */
  establish(token: string, account: Account): void {
    this.token.set(token);
    this.account.set(account);
    this.stored.set(TOKEN_KEY, token);
    this.stored.set(ACCOUNT_KEY, JSON.stringify(account));
  }

  /**
   * Adopts a token that arrived some other way — an access link, or a redeemed language handoff.
   *
   * The account record is dropped unless the token is the one it was stored beside: a different
   * token is a different account, and a stale name in the header is a lie about whose trials are
   * on screen.
   */
  adopt(token: string): void {
    if (token !== this.token()) this.forgetAccount();
    this.token.set(token);
    this.stored.set(TOKEN_KEY, token);
  }

  /** What the server said the name is, which is not necessarily what was typed (trim, collapse). */
  rememberAccount(account: Account): void {
    this.account.set(account);
    this.stored.set(ACCOUNT_KEY, JSON.stringify(account));
  }

  /**
   * Drops the token. Not a logout in any recoverable sense — the server holds only a hash (D9),
   * so a token forgotten here without the link written down somewhere is an account nobody can
   * reach again. The interface says so before calling this.
   */
  signOut(): void {
    this.token.set(null);
    this.forgetAccount();
    this.stored.remove(TOKEN_KEY);
  }

  private forgetAccount(): void {
    this.account.set(null);
    this.stored.remove(ACCOUNT_KEY);
  }

  /**
   * Reads `#t=…` and wipes it from the address bar in the same turn.
   *
   * `replaceState` rather than `pushState`: a back button that returns to the URL carrying the
   * secret would put it back on screen, which is what the fragment scheme exists to prevent.
   */
  private takeFragment(): string | null {
    const hash = location.hash.startsWith('#') ? location.hash.slice(1) : location.hash;
    if (hash === '') return null;

    const token = new URLSearchParams(hash).get('t');
    if (token === null || token === '') return null;

    history.replaceState(null, '', location.pathname + location.search);
    return token;
  }
}

function parseAccount(raw: string | null): Account | null {
  if (raw === null) return null;
  try {
    const parsed: unknown = JSON.parse(raw);
    if (typeof parsed !== 'object' || parsed === null) return null;
    const { publicId, name } = parsed as Partial<Account>;
    return typeof publicId === 'string' && typeof name === 'string' ? { publicId, name } : null;
  } catch {
    // A record this version cannot read is not worth a broken page. The token is stored
    // separately and survives, so the account still plays — it just shows its public id until
    // the next thing the server tells us about it.
    return null;
  }
}

/**
 * `localStorage`, or nothing at all.
 *
 * Private windows and locked-down profiles throw on the very first access rather than returning
 * null. Failing open costs the reload — the session lives for as long as the tab does — and the
 * access link is on screen the whole time, so the user can still keep their account. Failing
 * closed would mean a blank page for a setting that has nothing to do with this site.
 */
class Store {
  private readonly memory = new Map<string, string>();
  private readonly persistent = probe();

  get(key: string): string | null {
    if (!this.persistent) return this.memory.get(key) ?? null;
    try {
      return localStorage.getItem(key);
    } catch {
      return this.memory.get(key) ?? null;
    }
  }

  set(key: string, value: string): void {
    this.memory.set(key, value);
    if (!this.persistent) return;
    try {
      localStorage.setItem(key, value);
    } catch {
      // Quota, or storage revoked mid-session. The in-memory copy above already took it.
    }
  }

  remove(key: string): void {
    this.memory.delete(key);
    if (!this.persistent) return;
    try {
      localStorage.removeItem(key);
    } catch {
      // As above.
    }
  }
}

function probe(): boolean {
  try {
    const key = 'vriltrainer.probe';
    localStorage.setItem(key, '1');
    localStorage.removeItem(key);
    return true;
  } catch {
    return false;
  }
}
