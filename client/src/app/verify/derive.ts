/**
 * Trial derivation — the browser half of D7.
 *
 * This deliberately does not share code with the server. A client that verifies the server using
 * the server's own implementation verifies nothing; two independent implementations agreeing on
 * `shared/vectors/derivation.json` are evidence that both follow the written specification.
 *
 * Every constant here is normative. See `shared/vectors/README.md`.
 */

import { framed, le64 } from './framing';

export const SET_SIZE = 8;

const MAX_U64 = (1n << 64n) - 1n;

/** `SHA-256(seed ‖ LE64(counter))`, consumed as little-endian 64-bit words. */
export class Stream {
  private block = new Uint8Array(32);
  private counter = 0n;
  private offset = 32; // forces a block on the first word

  private constructor(private readonly seed: Uint8Array) {}

  static async create(sServer: Uint8Array, sClient: Uint8Array): Promise<Stream> {
    return new Stream(await framed([sServer, sClient]));
  }

  private async nextWord(): Promise<bigint> {
    if (this.offset >= 32) {
      const buf = new Uint8Array(40);
      buf.set(this.seed, 0);
      buf.set(le64(this.counter), 32);
      this.block = new Uint8Array(await crypto.subtle.digest('SHA-256', buf));
      this.counter += 1n;
      this.offset = 0;
    }
    let w = 0n;
    for (let i = 7; i >= 0; i--) w = (w << 8n) | BigInt(this.block[this.offset + i]);
    this.offset += 8;
    return w;
  }

  /**
   * Uniform integer in [0, m) by rejection sampling. The bound is `floor(2^64/m)*m`, expressed
   * relative to `2^64 - 1` because neither fits in 64 bits. Nothing is rejected when m divides 2^64.
   */
  async below(m: bigint): Promise<bigint> {
    if (m <= 0n) throw new Error('below(0) is undefined');
    const twoPow64ModM = ((MAX_U64 % m) + 1n) % m;
    for (;;) {
      const w = await this.nextWord();
      if (twoPow64ModM === 0n || w < MAX_U64 - twoPow64ModM + 1n) return w % m;
    }
  }
}

export interface Draw {
  /** Category indices, in selection order, never sorted. */
  chosenCategories: number[];
  /** Manifest image indices, in selection order, one per chosen category. */
  selectedImages: number[];
  /** Which of the eight is the target, in *selection* order. */
  targetSlot: number;
  /** `displayOrder[d]` is the selection index shown at display position d. */
  displayOrder: number[];
}

/**
 * The four steps of D22.
 *
 * `members[c]` holds the manifest indices of category c, in manifest order — the sorted manifest
 * filtered to that category, which is the only ordering that exists.
 */
export async function derive(
  sServer: Uint8Array,
  sClient: Uint8Array,
  members: number[][],
): Promise<Draw> {
  const k = members.length;
  if (k < SET_SIZE) throw new Error(`pool has ${k} categories, at least ${SET_SIZE} are required`);
  const empty = members.findIndex((m) => m.length === 0);
  if (empty >= 0) throw new Error(`category index ${empty} holds no images`);

  const st = await Stream.create(sServer, sClient);

  // 1 — eight distinct categories, partial Fisher-Yates.
  const cats = Array.from({ length: k }, (_, i) => i);
  for (let i = 0; i < SET_SIZE; i++) {
    const j = i + Number(await st.below(BigInt(k - i)));
    [cats[i], cats[j]] = [cats[j], cats[i]];
  }
  const chosenCategories = cats.slice(0, SET_SIZE);

  // 2 — one image from each, in selection order.
  const selectedImages: number[] = [];
  for (let i = 0; i < SET_SIZE; i++) {
    const m = members[chosenCategories[i]];
    selectedImages.push(m[Number(await st.below(BigInt(m.length)))]);
  }

  // 3 — the target slot. Uniform over the eight shown, independent of category sizes. This is
  // what makes a size-proportional bias impossible rather than merely avoided.
  const targetSlot = Number(await st.below(BigInt(SET_SIZE)));

  // 4 — display order, descending Fisher-Yates. The direction is normative: ascending would
  // consume the stream differently and produce another permutation from the same seed.
  const displayOrder = Array.from({ length: SET_SIZE }, (_, i) => i);
  for (let i = SET_SIZE - 1; i >= 1; i--) {
    const j = Number(await st.below(BigInt(i + 1)));
    [displayOrder[i], displayOrder[j]] = [displayOrder[j], displayOrder[i]];
  }

  return { chosenCategories, selectedImages, targetSlot, displayOrder };
}

/** Members per category, from a manifest's category and image lists. */
export function membersOf(
  categories: string[],
  images: { id: string; category: string }[],
): number[][] {
  return categories.map((c) =>
    images.map((e, i) => (e.category === c ? i : -1)).filter((i) => i >= 0),
  );
}
