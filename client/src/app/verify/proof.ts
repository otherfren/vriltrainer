/**
 * The recomputation the product is selling.
 *
 * Everything here is built from values the **server** handed over — the commitment and pool hash
 * that travelled with the coordinate before anything was revealed, and the three secrets released
 * with the answer — and is then re-derived by this browser's own implementation of the written
 * specification. Nothing is remembered from a local simulation, because there is no longer one.
 *
 * The panel shows the whole of it: every field, the full length-prefixed preimage in hex, the
 * digest, and all four steps of D22. Abbreviating the preimage would be the one place on the site
 * where a reader is asked to take a hash on faith.
 *
 * The single value this browser contributes is `s_client`, and it is checked rather than trusted:
 * see `RECOMPUTED_S_CLIENT` below.
 */

import { commitment, frame, framed, le64, toHex, utf8 } from './framing';
import { Draw, derive, membersOf } from './derive';
import { PoolManifest, manifestHash, validateManifest } from './manifest';

/** One line of the recomputation, shown in full. */
export interface ProofPart {
  label: string;
  value: string;
  note?: string;
}

/** One thing the recomputation either agrees with the server about, or does not. */
export interface ProofCheck {
  label: string;
  detail: string;
  ok: boolean;
}

export interface Proof {
  /** The inputs, revealed only after the answer. */
  inputs: ProofPart[];
  /** The pool the draw ran against, and how it was tied to this trial. */
  poolParts: ProofPart[];

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

  /** Every claim this recomputation makes about the server, and whether it holds. */
  checks: ProofCheck[];
  /** True only when every check above holds. */
  verified: boolean;
}

/** What the commit published, before anything was revealed. */
export interface SealedTrial {
  coordinate: string;
  commitment: string;
  poolVersion: number;
  poolManifestHash: string;
}

/** What the reveal produced: this browser's contribution, and the eight it was answered with. */
export interface RevealedTrial {
  /** The bytes `crypto.getRandomValues` put in this browser, before they were sent. */
  sent: Uint8Array;
  /** Exactly eight identifiers, in the order they were displayed. */
  images: string[];
}

/** The three secrets, released together with the verdict. */
export interface AnsweredTrial {
  sServer: Uint8Array;
  sClient: Uint8Array;
  nonce: Uint8Array;
  target: string;
}

export async function verifyTrial(
  sealed: SealedTrial,
  revealed: RevealedTrial,
  answered: AnsweredTrial,
  manifest: PoolManifest,
): Promise<Proof> {
  const checks: ProofCheck[] = [];

  // ---- the pool -----------------------------------------------------------------------------
  // First, because everything below draws against its indices. A manifest nobody checked makes
  // the rest of this page a proof that the server agrees with itself.
  const structural = validateManifest(manifest);
  const recomputedPoolHash = await manifestHash(manifest.categories, manifest.images);

  checks.push({
    label: 'Pool-Manifest',
    detail:
      structural !== null
        ? `Das Manifest ist formal ungültig: ${structural}`
        : recomputedPoolHash !== manifest.manifest_hash
          ? 'Der neu berechnete Hash des Manifests weicht von dem ab, den es selbst nennt.'
          : recomputedPoolHash !== sealed.poolManifestHash
            ? 'Das gelieferte Manifest ist nicht das, auf das diese Sitzung festgelegt wurde.'
            : `${manifest.images.length} Bilder in ${manifest.categories.length} Kategorien, Hash neu berechnet und identisch mit dem, der schon zur Koordinate mitgeliefert wurde.`,
    ok:
      structural === null &&
      recomputedPoolHash === manifest.manifest_hash &&
      recomputedPoolHash === sealed.poolManifestHash,
  });

  // ---- the commitment -----------------------------------------------------------------------
  const coordBytes = utf8(sealed.coordinate);
  const commitmentPreimage = frame([answered.sServer, answered.nonce, coordBytes]);
  const commitmentDigest = await commitment(
    answered.sServer,
    answered.nonce,
    sealed.coordinate,
  );
  const matches = commitmentDigest === sealed.commitment;

  checks.push({
    label: 'Versiegelung',
    detail: matches
      ? 'Aus den jetzt offengelegten Werten entsteht genau die Zahl, die vor deiner Wahl auf dem Schirm stand.'
      : 'Die Zahl von vorher lässt sich aus den offengelegten Werten nicht reproduzieren.',
    ok: matches,
  });

  // ---- the client's own half ------------------------------------------------------------------
  // RECOMPUTED_S_CLIENT: the server echoes `s_client` back although this browser produced it, so
  // the panel checks one payload instead of half a payload and half its own memory (D3, SC-002).
  // That is only worth anything if the echo is compared to what was actually sent: a server free
  // to return a different `s_client` could pick the seed after seeing the choice, and every other
  // check on this page would still pass.
  const echoed = sameBytes(revealed.sent, answered.sClient);
  checks.push({
    label: 'Dein Zufallsanteil',
    detail: echoed
      ? 'Die 32 Bytes aus deinem Browser kommen unverändert zurück — der Seed ist zweiseitig.'
      : 'Der Server gibt andere 32 Bytes zurück, als dein Browser geschickt hat.',
    ok: echoed,
  });

  // ---- the draw -------------------------------------------------------------------------------
  const seedPreimage = frame([answered.sServer, answered.sClient]);
  const seed = toHex(await framed([answered.sServer, answered.sClient]));

  const members = membersOf(manifest.categories, manifest.images);
  let draw: Draw | null = null;
  let drawError: string | null = null;
  try {
    draw = await derive(answered.sServer, answered.sClient, members);
  } catch (e) {
    drawError = e instanceof Error ? e.message : String(e);
  }

  let derivedImages: string[] = [];
  let derivedTarget: string | null = null;
  if (draw !== null) {
    const d = draw;
    derivedImages = d.displayOrder.map((sel) => manifest.images[d.selectedImages[sel]].id);
    derivedTarget = manifest.images[d.selectedImages[d.targetSlot]].id;
  }

  checks.push({
    label: 'Die acht Bilder',
    detail:
      drawError !== null
        ? `Die Herleitung lief nicht durch: ${drawError}`
        : sameOrder(derivedImages, revealed.images)
          ? 'Der Seed erzeugt genau diese acht Bilder, in genau dieser Reihenfolge.'
          : 'Der Seed erzeugt eine andere Auswahl oder eine andere Reihenfolge als die gezeigte.',
    ok: drawError === null && sameOrder(derivedImages, revealed.images),
  });

  checks.push({
    label: 'Das Ziel',
    detail:
      derivedTarget === null
        ? 'Ohne Herleitung gibt es kein nachgerechnetes Ziel.'
        : derivedTarget === answered.target
          ? 'Das nachgerechnete Ziel ist dasselbe, gegen das deine Wahl gewertet wurde.'
          : 'Das nachgerechnete Ziel ist ein anderes als das gewertete.',
    ok: derivedTarget !== null && derivedTarget === answered.target,
  });

  return {
    inputs: [
      { label: 's_server', value: toHex(answered.sServer), note: '32 Bytes, vom Server' },
      { label: 'nonce', value: toHex(answered.nonce), note: '32 Bytes, vom Server' },
      {
        label: 's_client',
        value: toHex(answered.sClient),
        note: echoed
          ? '32 Bytes aus deinem Browser, vom Server unverändert zurückgegeben'
          : `32 Bytes vom Server — dein Browser hatte ${toHex(revealed.sent)} geschickt`,
      },
      {
        label: 'Koordinate',
        value: sealed.coordinate,
        note: `${coordBytes.length} Bytes UTF-8 · ${toHex(coordBytes)}`,
      },
    ],

    poolParts: [
      { label: 'Pool-Version', value: `v${manifest.version}`, note: poolNote(manifest) },
      {
        label: 'Hash laut Manifest',
        value: manifest.manifest_hash,
        note: 'Über die sortierten (id, category)-Paare, Kategorien zuerst',
      },
      {
        label: 'Hier neu berechnet',
        value: recomputedPoolHash,
      },
      {
        label: 'Zur Koordinate mitgeliefert',
        value: sealed.poolManifestHash,
        note: 'Stand fest, bevor irgendetwas aufgedeckt wurde',
      },
    ],

    commitmentParts: [
      { label: 'LE64(32)', value: toHex(le64(32n)), note: 'Länge von s_server' },
      { label: 's_server', value: toHex(answered.sServer) },
      { label: 'LE64(32)', value: toHex(le64(32n)), note: 'Länge des nonce' },
      { label: 'nonce', value: toHex(answered.nonce) },
      {
        label: `LE64(${coordBytes.length})`,
        value: toHex(le64(BigInt(coordBytes.length))),
        note: 'Länge der Koordinate',
      },
      {
        label: 'Koordinate',
        value: toHex(coordBytes),
        note: `"${sealed.coordinate}" als UTF-8`,
      },
    ],
    commitmentPreimage: toHex(commitmentPreimage),
    commitmentDigest,
    published: sealed.commitment,
    matches,

    seedParts: [
      { label: 'LE64(32)', value: toHex(le64(32n)) },
      { label: 's_server', value: toHex(answered.sServer) },
      { label: 'LE64(32)', value: toHex(le64(32n)) },
      { label: 's_client', value: toHex(answered.sClient) },
    ],
    seedPreimage: toHex(seedPreimage),
    seed,

    steps: draw === null ? [] : steps(draw, manifest, members, revealed.images),

    checks,
    verified: checks.every((c) => c.ok),
  };
}

/** The four steps of D22, named with the manifest's own identifiers. */
function steps(
  draw: Draw,
  manifest: PoolManifest,
  members: number[][],
  shown: string[],
): ProofPart[] {
  const category = (i: number) => manifest.categories[i];
  const shownAt = draw.displayOrder.indexOf(draw.targetSlot);

  return [
    {
      label: '1 · Acht Kategorien',
      value: draw.chosenCategories.map(category).join(', '),
      note:
        `Indizes ${draw.chosenCategories.join(', ')} aus ${manifest.categories.length} Kategorien, ` +
        'in Auswahlreihenfolge, partielles Fisher-Yates',
    },
    {
      label: '2 · Ein Bild je Kategorie',
      value: draw.selectedImages.map((i) => manifest.images[i].id).join(', '),
      note:
        'Manifest-Indizes ' +
        draw.selectedImages.join(', ') +
        ' — gezogen aus ' +
        draw.chosenCategories.map((c) => `${category(c)}: ${members[c].length}`).join(', '),
    },
    {
      label: '3 · Zielplatz',
      value: `${draw.targetSlot}`,
      note: 'Gleichverteilt über die acht, unabhängig von der Kategoriegröße',
    },
    {
      label: '4 · Anzeigereihenfolge',
      value: draw.displayOrder.join(', '),
      note:
        `Absteigendes Fisher-Yates; Position ${shownAt} zeigt das Ziel. ` +
        `Gezeigt wurden ${shown.length} Bilder.`,
    },
  ];
}

/** `created` and `count` are only in a published manifest file, never in a served in-memory copy. */
function poolNote(manifest: PoolManifest): string {
  const parts = [`${manifest.images.length} Bilder`, `${manifest.categories.length} Kategorien`];
  if (manifest.created !== undefined) parts.push(`veröffentlicht ${manifest.created}`);
  return parts.join(' · ');
}

/** Constant in the number of bytes compared. Not a secret, but neither is it worth a special case. */
function sameBytes(a: Uint8Array, b: Uint8Array): boolean {
  if (a.length !== b.length) return false;
  let diff = 0;
  for (let i = 0; i < a.length; i++) diff |= a[i] ^ b[i];
  return diff === 0;
}

function sameOrder(a: string[], b: string[]): boolean {
  return a.length === b.length && a.every((x, i) => x === b[i]);
}
