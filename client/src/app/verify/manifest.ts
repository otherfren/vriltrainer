/**
 * The pool manifest, and its hash recomputed in the browser — the TypeScript half of
 * `server/src/pool.rs`.
 *
 * Written from `contracts/pool-manifest.md` rather than from the server's code, for the same
 * reason `derive.ts` is: a client that checks the server with the server's own implementation
 * checks nothing.
 *
 * Why this matters for the proof panel. The derivation draws against *manifest indices*, so the
 * eight images a seed names depend entirely on which manifest is in front of you. Recomputing a
 * trial against a manifest fetched from the same server, without checking it, proves only that
 * the server is self-consistent. Rehashing it and comparing the result to the
 * `pool_manifest_hash` the **commit** carried — which was published before the reveal — is what
 * ties the recomputation to the trial that was actually sealed (D3, D5, D22).
 */

import { le64, toHex, utf8 } from './framing';

export interface ManifestImage {
  id: string;
  category: string;
}

export interface PoolManifest {
  version: number;
  /** Present in a published manifest file; absent when the server serves its in-memory copy. */
  created?: string;
  count?: number;
  categories: string[];
  /** Sorted ascending by id. **This order is the index the derivation draws against.** */
  images: ManifestImage[];
  manifest_hash: string;
}

/**
 * `SHA-256` over the sorted `(id, category)` pairs, categories first.
 *
 * The layout is not `framed()`: the two collection lengths are **counts**, written as bare
 * `LE64`, while each string is length-prefixed. Copied deliberately rather than folded into the
 * framing helper, because it is a separate normative layout and making the two share a function
 * would let a change to one silently redefine the other.
 *
 * The category is inside the hash on purpose. Hashing identifiers alone would let a category be
 * reassigned without the manifest appearing to change, silently altering every future derivation.
 */
export async function manifestHash(
  categories: string[],
  images: ManifestImage[],
): Promise<string> {
  const parts: Uint8Array[] = [le64(BigInt(categories.length))];
  for (const c of categories) {
    const bytes = utf8(c);
    parts.push(le64(BigInt(bytes.length)), bytes);
  }
  parts.push(le64(BigInt(images.length)));
  for (const e of images) {
    const id = utf8(e.id);
    const category = utf8(e.category);
    parts.push(le64(BigInt(id.length)), id, le64(BigInt(category.length)), category);
  }

  let total = 0;
  for (const p of parts) total += p.length;
  const buf = new Uint8Array(total);
  let at = 0;
  for (const p of parts) {
    buf.set(p, at);
    at += p.length;
  }

  return 'sha256:' + toHex(new Uint8Array(await crypto.subtle.digest('SHA-256', buf)));
}

/**
 * The structural rules of the format, checked before anything is derived against it.
 *
 * A manifest that is out of order or has an image in an undeclared category would still produce
 * *some* eight images, silently different from the ones the server drew. Refusing here turns
 * that into a visible failure of the proof rather than an unexplained mismatch further down.
 */
export function validateManifest(m: PoolManifest): string | null {
  for (let i = 1; i < m.images.length; i++) {
    if (!(m.images[i - 1].id < m.images[i].id)) {
      return 'images are not sorted ascending by id, or contain duplicates';
    }
  }
  for (const e of m.images) {
    if (!m.categories.includes(e.category)) {
      return `image ${e.id} has undeclared category ${e.category}`;
    }
  }
  return null;
}
