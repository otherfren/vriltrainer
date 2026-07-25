import { Injectable, inject } from '@angular/core';
import { PoolManifest } from '../verify/manifest';
import { SessionService } from './session.service';

/**
 * The one place this application talks to the Rust service described in `contracts/http-api.md`.
 *
 * Two rules hold everywhere below.
 *
 * **Nothing is computed here that the server is supposed to prove.** An earlier version of this
 * file generated `s_server`, the nonce and the coordinate in the browser and ran the derivation
 * against a pool of eight demo images. That was honest only because a badge said so; a client
 * that can produce a trial by itself cannot be told apart from one that is checking a server, and
 * the checking is the product. The single value this file still draws is `s_client`, which is the
 * browser's contribution by design (D3) — and the answer echoes it back so the proof panel can
 * verify even that.
 *
 * **The access token appears in exactly one header.** Never a path, never a query string, never a
 * log line, never an error body (FR-006, D9). `SessionService` owns it; this file only attaches it.
 */

/** Bytes each side contributes to the seed. Fixed by the contract, not negotiated. */
const SEED_BYTES = 32;

/** A refusal the server spelled out, carrying the terse code its body contained. */
export class ApiError extends Error {
  constructor(
    readonly status: number,
    /** The server's machine-readable code, e.g. `hate`, `gone`, `too fast`. */
    readonly code: string,
  ) {
    super(`${status} ${code}`);
    this.name = 'ApiError';
  }
}

/** The network did not deliver an answer. Distinct from [`ApiError`], and the distinction matters:
 *  a request that never arrived may still have been executed. */
export class NetworkError extends Error {
  constructor(cause: unknown) {
    super('the request did not reach the server');
    this.name = 'NetworkError';
    this.cause = cause;
  }
}

// ---- wire shapes ------------------------------------------------------------------------------
// Named as the JSON names them, so a reader can hold this file and the contract side by side.

/** `GET /api/account`: who the bearer of this token is. */
export interface OwnAccount {
  public_id: string;
  /** Null after erasure, and after a refusal discarded what was submitted. */
  name: string | null;
  name_state: 'pending' | 'approved' | 'rejected' | 'erased';
}

export interface CreatedAccount {
  public_id: string;
  /** Handed over once and never again (FR-002, FR-005, D9). */
  access_token: string;
  /** The **stored** form — trimmed and collapsed — not necessarily what was typed. */
  name: string;
}

export interface TrialStart {
  trialId: string;
  coordinate: string;
  commitment: string;
  poolVersion: number;
  poolManifestHash: string;
  /** Opaque. It carries the trial's state, sealed by the server (D16). */
  token: string;
}

export interface Revealed {
  /** Exactly eight identifiers, in derived display order. */
  images: string[];
  /** The next token. The one that started the trial is spent. */
  token: string;
  /** The bytes this browser drew and sent, kept so the proof can check the echo. */
  sentSClient: Uint8Array;
}

export interface Answered {
  hit: boolean;
  target: string;
  sServer: Uint8Array;
  sClient: Uint8Array;
  nonce: Uint8Array;
  /** Where this trial's `RESOLVE` entry sits in the public record. */
  seq: number;
}

/** The band edges in force, reported by the server rather than compiled in (D26, FR-050). */
export interface RankBand {
  high: string;
  low: string;
  share: number;
}

export interface Thresholds {
  stats_unlock_at: number;
  eligibility_trials: number;
  eligibility_days: number;
  bands: RankBand[];
  block_size: number;
}

/**
 * `GET /api/stats/me`. Two shapes in one interface: everything past `abandoned` is absent until
 * the account has completed `unlocks_at` trials (D8).
 */
export interface MyStats {
  completed: number;
  abandoned: number;
  unlocks_at: number;
  thresholds: Thresholds;

  hits?: number;
  hit_rate?: number;
  /** The `n` the three inferential figures stand over, and the hits inside it (FR-019). */
  reported_trials?: number;
  reported_hits?: number;
  deviation?: number;
  by_chance_per_10k?: number;
  wilson_lower?: number;
  distinct_days?: number;
  eligible?: boolean;
  /** A band slug. Absent for the middle 60 % and for a band the population cannot fill yet. */
  rank?: string;
}

export interface AggregateStats {
  trials: number;
  hits: number;
  hit_rate: number;
  expected_rate: number;
  deviation: number;
  accounts: number;
  abandoned: number;
  tail_high: number;
  tail_low: number;
  /** What "markedly" means, in standard deviations, and over what minimum record. */
  tail_sigma: number;
  tail_min_trials: number;
  thresholds: Thresholds;
}

export interface BoardEntry {
  place: number;
  band?: string;
  /** The most recently approved name, or a fixed-length mask (FR-047, D25). */
  name: string;
  public_id: string;
  wilson_lower: number;
  completed: number;
  hit_rate: number;
  deviation: number;
}

export interface Board {
  eligible_accounts: number;
  bands_active: string[];
  ranks_updated_at: string;
  offset: number;
  limit: number;
  entries: BoardEntry[];
  thresholds: Thresholds;
}

@Injectable({ providedIn: 'root' })
export class ApiService {
  private readonly session = inject(SessionService);

  /**
   * Manifests already fetched, by version.
   *
   * A manifest is immutable once published — a change cuts a new version (FR-012) — so this can
   * be held for the life of the page without ever going stale. It is also several hundred
   * entries, and re-fetching it per trial would put the pool on the wire more often than the
   * trials themselves.
   */
  private readonly manifests = new Map<number, Promise<PoolManifest>>();

  // ---- account ------------------------------------------------------------------------------

  /**
   * Creates the account and, with it, the only credential that will ever exist for it.
   *
   * The token is handed to [`SessionService`] immediately rather than returned, so there is no
   * path by which a caller holds it and forgets to store it — that would be an account lost
   * between two lines of code, with no recovery (FR-005).
   */
  async createAccount(name: string): Promise<CreatedAccount> {
    const created = await this.request<CreatedAccount>('POST', '/api/account', { name }, false);
    this.session.establish(created.access_token, {
      publicId: created.public_id,
      name: created.name,
    });
    return created;
  }

  /**
   * Who this browser is playing as.
   *
   * The access link carries a capability and not an identity (D9) — a random 32 bytes from which
   * no name can be derived, deliberately, because a name in the fragment would travel into
   * bookmark titles and go stale the moment it changed. So a browser that arrived through one
   * knows it has an account and nothing else, and has to ask.
   */
  async whoami(): Promise<OwnAccount> {
    return this.request<OwnAccount>('GET', '/api/account');
  }

  // ---- the trial loop -----------------------------------------------------------------------

  /** Starts a trial. The server writes the `COMMIT` entry before it answers (FR-007, D3). */
  async startTrial(): Promise<TrialStart> {
    const body = await this.request<{
      trial_id: string;
      coordinate: string;
      commitment: string;
      pool_version: number;
      pool_manifest_hash: string;
      token: string;
    }>('POST', '/api/trial', {});

    return {
      trialId: body.trial_id,
      coordinate: body.coordinate,
      commitment: body.commitment,
      poolVersion: body.pool_version,
      poolManifestHash: body.pool_manifest_hash,
      token: body.token,
    };
  }

  /**
   * Contributes this browser's half of the seed and receives the eight candidates.
   *
   * The randomness is drawn here, at the moment it is sent, and nowhere else. Drawing it when the
   * trial starts would leave it sitting in memory across the whole sealed phase for no benefit;
   * reusing one value across trials would make every trial's draw a function of the first.
   */
  async reveal(token: string): Promise<Revealed> {
    const sentSClient = randomBytes(SEED_BYTES);
    const body = await this.request<{ images: string[]; token: string }>(
      'POST',
      '/api/trial/reveal',
      { token, s_client: toBase64(sentSClient) },
    );
    return { images: body.images, token: body.token, sentSClient };
  }

  /** Submits the choice and receives the verdict together with the three secrets (FR-010). */
  async answer(token: string, chosen: string): Promise<Answered> {
    const body = await this.request<{
      hit: boolean;
      target: string;
      s_server: string;
      s_client: string;
      nonce: string;
      seq: number;
    }>('POST', '/api/trial/answer', { token, chosen });

    return {
      hit: body.hit,
      target: body.target,
      sServer: fromBase64(body.s_server),
      sClient: fromBase64(body.s_client),
      nonce: fromBase64(body.nonce),
      seq: body.seq,
    };
  }

  /** The published image list for a pool version. Required by anyone recomputing a trial (D5). */
  manifest(version: number): Promise<PoolManifest> {
    const held = this.manifests.get(version);
    if (held !== undefined) return held;

    // The promise is cached, not the value, so eight components asking at once make one request.
    // A failed fetch is evicted: a manifest that could not be loaded once must be retryable, or
    // one dropped connection costs the proof panel for the rest of the session.
    const pending = this.request<PoolManifest>(
      'GET',
      `/api/pool/${version}/manifest`,
      undefined,
      false,
    ).catch((e: unknown) => {
      this.manifests.delete(version);
      throw e;
    });
    this.manifests.set(version, pending);
    return pending;
  }

  // ---- figures ------------------------------------------------------------------------------

  myStats(): Promise<MyStats> {
    return this.request<MyStats>('GET', '/api/stats/me');
  }

  aggregate(): Promise<AggregateStats> {
    return this.request<AggregateStats>('GET', '/api/stats/aggregate', undefined, false);
  }

  leaderboard(offset: number, limit: number): Promise<Board> {
    return this.request<Board>(
      'GET',
      `/api/leaderboard?offset=${offset}&limit=${limit}`,
      undefined,
      false,
    );
  }

  // ---- the transport ------------------------------------------------------------------------

  /**
   * One request, one place where the token is attached and where a failure is classified.
   *
   * `authenticated` is explicit rather than inferred from whether a token happens to exist. The
   * public endpoints must stay readable by a browser that has no account, and an `Authorization`
   * header sent to them would make the leaderboard's cacheability depend on who is looking.
   */
  private async request<T>(
    method: string,
    path: string,
    body?: unknown,
    authenticated = true,
  ): Promise<T> {
    const headers: Record<string, string> = { Accept: 'application/json' };
    if (body !== undefined) headers['Content-Type'] = 'application/json';
    if (authenticated) {
      const token = this.session.token();
      if (token === null) throw new ApiError(401, 'unauthorized');
      headers['Authorization'] = `Bearer ${token}`;
    }

    let response: Response;
    try {
      response = await fetch(path, {
        method,
        headers,
        body: body === undefined ? undefined : JSON.stringify(body),
        // The token is a header, not a cookie. Sending credentials would attach nothing and
        // widen what a cross-origin response is allowed to do.
        credentials: 'omit',
        redirect: 'error',
      });
    } catch (e) {
      // A network failure is not a refusal. A `POST /api/trial/answer` that dies here may well
      // have been executed, and the caller has to say so rather than score a miss.
      throw new NetworkError(e);
    }

    if (!response.ok) {
      throw new ApiError(response.status, await errorCode(response));
    }
    if (response.status === 204) return undefined as T;
    return (await response.json()) as T;
  }
}

/** Every error body in this API is `{ "error": "<terse code>" }`. Anything else is the status. */
async function errorCode(response: Response): Promise<string> {
  try {
    const body: unknown = await response.json();
    if (typeof body === 'object' && body !== null) {
      const { error } = body as { error?: unknown };
      if (typeof error === 'string') return error;
    }
  } catch {
    // A proxy's HTML error page, or an empty body. The status carries the meaning either way.
  }
  return `http ${response.status}`;
}

function randomBytes(n: number): Uint8Array {
  const b = new Uint8Array(n);
  crypto.getRandomValues(b);
  return b;
}

/** Standard base64 with padding, which is what the server's `STANDARD` engine reads and writes. */
function toBase64(b: Uint8Array): string {
  let s = '';
  for (const byte of b) s += String.fromCharCode(byte);
  return btoa(s);
}

function fromBase64(s: string): Uint8Array {
  const raw = atob(s);
  const out = new Uint8Array(raw.length);
  for (let i = 0; i < raw.length; i++) out[i] = raw.charCodeAt(i);
  return out;
}
