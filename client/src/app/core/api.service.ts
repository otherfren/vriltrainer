import { Injectable } from '@angular/core';

export interface TrialStart {
  trialId: string;
  coordinate: string;
  commitment: string;
  poolVersion: number;
  poolManifestHash: string;
  token: string;
}

export interface TrialReveal {
  images: string[];
  token: string;
}

export interface TrialAnswer {
  hit: boolean;
  target: string;
  sServer: string;
  sClient: string;
  nonce: string;
  seq: number;
}

/**
 * Talks to the Rust service described in `contracts/http-api.md`.
 *
 * The demo path below runs the real derivation in the browser so the interface can be exercised
 * before the server is wired up. It is deliberately obvious which mode is active: a demo trial
 * says so on screen rather than pretending to be evidence of anything.
 */
@Injectable({ providedIn: 'root' })
export class ApiService {
  readonly demoMode = true;

  private randomHex(bytes: number): string {
    const b = new Uint8Array(bytes);
    crypto.getRandomValues(b);
    return Array.from(b)
      .map((x) => x.toString(16).padStart(2, '0'))
      .join('');
  }

  /** A coordinate is an arbitrary label. It encodes nothing (research.md R6). */
  private coordinate(): string {
    const d = () => Math.floor(Math.random() * 10000).toString().padStart(4, '0');
    return `${d()}-${d()}`;
  }

  newTrial(): TrialStart {
    return {
      trialId: this.randomHex(8),
      coordinate: this.coordinate(),
      commitment: 'sha256:' + this.randomHex(32),
      poolVersion: 1,
      poolManifestHash: 'sha256:' + this.randomHex(32),
      token: this.randomHex(24),
    };
  }
}
