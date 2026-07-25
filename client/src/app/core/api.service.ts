import { Injectable } from '@angular/core';
import { commitment, frame, framed, fromHex, le64, toHex, utf8 } from '../verify/framing';
import { derive, Draw } from '../verify/derive';

export interface TrialStart {
  trialId: string;
  coordinate: string;
  commitment: string;
  poolVersion: number;
  poolManifestHash: string;
  token: string;
}

export interface Candidate {
  id: string;
  category: string;
  src: string;
}

/** One line of the recomputation, shown in full. */
export interface ProofPart {
  label: string;
  value: string;
  note?: string;
}

export interface Proof {
  /** The three inputs to the commitment, revealed only after the answer. */
  inputs: ProofPart[];
  /** The framed preimage, segment by segment, then whole. */
  commitmentParts: ProofPart[];
  commitmentPreimage: string;
  commitmentDigest: string;
  published: string;
  matches: boolean;

  /** The draw: seed, then the four steps of D22. */
  seedParts: ProofPart[];
  seedPreimage: string;
  seed: string;
  steps: ProofPart[];
}

export interface Resolution {
  /** The eight candidates in display order. */
  candidates: Candidate[];
  /** Which display position holds the target. */
  targetSlot: number;
  proof: Proof;
}

/**
 * The demo pool: eight categories with one image each.
 *
 * A real pool is 500+ curated photographs across many categories, and step 2 of the derivation
 * picks one image from each chosen category. Here that step has nothing to choose, and the page
 * says so — a demo that quietly looked like the real thing would be the one dishonest surface on
 * a site whose entire promise is that you can check it.
 */
const DEMO_POOL: Candidate[] = [
  { id: 'demo-01', category: 'Landschaft', src: 'demo/target-1.svg' },
  { id: 'demo-02', category: 'Bauwerk', src: 'demo/target-2.svg' },
  { id: 'demo-03', category: 'Tier', src: 'demo/target-3.svg' },
  { id: 'demo-04', category: 'Objekt', src: 'demo/target-4.svg' },
  { id: 'demo-05', category: 'Fahrzeug', src: 'demo/target-5.svg' },
  { id: 'demo-06', category: 'Pflanze', src: 'demo/target-6.svg' },
  { id: 'demo-07', category: 'Werkzeug', src: 'demo/target-7.svg' },
  { id: 'demo-08', category: 'Himmelskörper', src: 'demo/target-8.svg' },
];

/** Secrets the server would hold until the answer is in. */
interface Sealed {
  sServer: Uint8Array;
  nonce: Uint8Array;
  coordinate: string;
  commitment: string;
}

/**
 * Talks to the Rust service described in `contracts/http-api.md`.
 *
 * The demo path runs the real derivation in the browser so the interface can be exercised before
 * the server is wired up: the commitment is genuinely `framed(s_server, nonce, coordinate)`, the
 * eight candidates and the target genuinely come from `derive()`, and the proof panel genuinely
 * recomputes both. It is deliberately obvious which mode is active.
 */
@Injectable({ providedIn: 'root' })
export class ApiService {
  readonly demoMode = true;
  readonly poolNote = 'Demo-Pool: acht Kategorien mit je einem Bild.';

  private sealed = new Map<string, Sealed>();

  private randomBytes(n: number): Uint8Array {
    const b = new Uint8Array(n);
    crypto.getRandomValues(b);
    return b;
  }

  /** A coordinate is an arbitrary label. It encodes nothing (research.md R6). */
  private coordinate(): string {
    const d = () =>
      Math.floor(Math.random() * 10000)
        .toString()
        .padStart(4, '0');
    return `${d()}-${d()}`;
  }

  async newTrial(): Promise<TrialStart> {
    const sServer = this.randomBytes(32);
    const nonce = this.randomBytes(32);
    const coordinate = this.coordinate();
    const trialId = toHex(this.randomBytes(8));

    // The real thing, not a random-looking string: this is what the reveal has to reproduce.
    const c = await commitment(sServer, nonce, coordinate);
    this.sealed.set(trialId, { sServer, nonce, coordinate, commitment: c });

    return {
      trialId,
      coordinate,
      commitment: c,
      poolVersion: 1,
      poolManifestHash: 'sha256:' + toHex(await framed([utf8('demo-pool-v1')])),
      token: toHex(this.randomBytes(24)),
    };
  }

  /**
   * The reveal. The client contributes `s_client` here, which is what stops the server from
   * choosing the target on its own — the seed is `framed(s_server, s_client)` and neither side
   * knows the other's half in time to steer it.
   */
  async reveal(trialId: string): Promise<Resolution> {
    const s = this.sealed.get(trialId);
    if (!s) throw new Error(`no sealed trial ${trialId}`);

    const sClient = this.randomBytes(32);
    const members = DEMO_POOL.map((_, i) => [i]);
    const draw = await derive(s.sServer, sClient, members);

    const candidates = draw.displayOrder.map((sel) => DEMO_POOL[draw.selectedImages[sel]]);
    const targetSlot = draw.displayOrder.indexOf(draw.targetSlot);

    return {
      candidates,
      targetSlot,
      proof: await this.proof(s, sClient, draw),
    };
  }

  private async proof(s: Sealed, sClient: Uint8Array, draw: Draw): Promise<Proof> {
    const coordBytes = utf8(s.coordinate);
    const preimage = frame([s.sServer, s.nonce, coordBytes]);
    const digest = 'sha256:' + toHex(await framed([s.sServer, s.nonce, coordBytes]));

    const seedPreimage = frame([s.sServer, sClient]);
    const seedDigest = toHex(await framed([s.sServer, sClient]));

    const cat = (i: number) => DEMO_POOL[i].category;

    return {
      inputs: [
        { label: 's_server', value: toHex(s.sServer), note: '32 Bytes, vom Server' },
        { label: 'nonce', value: toHex(s.nonce), note: '32 Bytes, vom Server' },
        { label: 's_client', value: toHex(sClient), note: '32 Bytes, aus deinem Browser' },
        {
          label: 'Koordinate',
          value: s.coordinate,
          note: `${coordBytes.length} Bytes UTF-8 · ${toHex(coordBytes)}`,
        },
      ],

      commitmentParts: [
        { label: 'LE64(32)', value: toHex(le64(32n)), note: 'Länge von s_server' },
        { label: 's_server', value: toHex(s.sServer) },
        { label: 'LE64(32)', value: toHex(le64(32n)), note: 'Länge des nonce' },
        { label: 'nonce', value: toHex(s.nonce) },
        {
          label: `LE64(${coordBytes.length})`,
          value: toHex(le64(BigInt(coordBytes.length))),
          note: 'Länge der Koordinate',
        },
        { label: 'Koordinate', value: toHex(coordBytes), note: `"${s.coordinate}" als UTF-8` },
      ],
      commitmentPreimage: toHex(preimage),
      commitmentDigest: digest,
      published: s.commitment,
      matches: digest === s.commitment,

      seedParts: [
        { label: 'LE64(32)', value: toHex(le64(32n)) },
        { label: 's_server', value: toHex(s.sServer) },
        { label: 'LE64(32)', value: toHex(le64(32n)) },
        { label: 's_client', value: toHex(sClient) },
      ],
      seedPreimage: toHex(seedPreimage),
      seed: seedDigest,

      steps: [
        {
          label: '1 · Acht Kategorien',
          value: draw.chosenCategories.map(cat).join(', '),
          note: `Indizes ${draw.chosenCategories.join(', ')} in Auswahlreihenfolge, partielles Fisher-Yates`,
        },
        {
          label: '2 · Ein Bild je Kategorie',
          value: draw.selectedImages.map((i) => DEMO_POOL[i].id).join(', '),
          note: 'Im Demo-Pool hat jede Kategorie genau ein Bild — dieser Schritt hat hier nichts zu wählen',
        },
        {
          label: '3 · Zielplatz',
          value: `${draw.targetSlot}`,
          note: 'Gleichverteilt über die acht, unabhängig von der Kategoriegröße',
        },
        {
          label: '4 · Anzeigereihenfolge',
          value: draw.displayOrder.join(', '),
          note: 'Absteigendes Fisher-Yates; Position ' + draw.displayOrder.indexOf(draw.targetSlot) + ' zeigt das Ziel',
        },
      ],
    };
  }

  /** Kept for the conformance harness and anything that needs the raw seed digest. */
  async seedHex(sServer: string, sClient: string): Promise<string> {
    return toHex(await framed([fromHex(sServer), fromHex(sClient)]));
  }
}
