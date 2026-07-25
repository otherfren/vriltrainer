/**
 * Unambiguous hashing of several fields — the TypeScript half of `server/src/framing.rs`.
 *
 * Plain concatenation is ambiguous whenever a field's length can vary: ("ab","c") and ("a","bc")
 * hash identically. Every field is length-prefixed instead, with the same rule the pool manifest
 * hash uses, so there is one thing to reason about rather than a case analysis.
 */

/** 64-bit little-endian, matching Rust's `u64::to_le_bytes`. */
export function le64(n: bigint): Uint8Array {
  const out = new Uint8Array(8);
  let v = n;
  for (let i = 0; i < 8; i++) {
    out[i] = Number(v & 0xffn);
    v >>= 8n;
  }
  return out;
}

export function utf8(s: string): Uint8Array {
  return new TextEncoder().encode(s);
}

export function toHex(b: Uint8Array): string {
  return Array.from(b)
    .map((x) => x.toString(16).padStart(2, '0'))
    .join('');
}

export function fromHex(s: string): Uint8Array {
  if (s.length % 2 !== 0) throw new Error(`hex string has odd length: ${s}`);
  const out = new Uint8Array(s.length / 2);
  for (let i = 0; i < out.length; i++) out[i] = parseInt(s.substr(i * 2, 2), 16);
  return out;
}

/**
 * `LE64(|p0|) ‖ p0 ‖ LE64(|p1|) ‖ p1 ‖ …` — the bytes that actually go into SHA-256.
 *
 * Split out from `framed` so the interface can show a reader the exact preimage rather than
 * asking them to take the digest on faith. There is still one definition of the layout.
 */
export function frame(parts: Uint8Array[]): Uint8Array {
  let total = 0;
  for (const p of parts) total += 8 + p.length;
  const buf = new Uint8Array(total);
  let o = 0;
  for (const p of parts) {
    buf.set(le64(BigInt(p.length)), o);
    o += 8;
    buf.set(p, o);
    o += p.length;
  }
  return buf;
}

/** `SHA-256( LE64(|p0|) ‖ p0 ‖ LE64(|p1|) ‖ p1 ‖ … )` */
export async function framed(parts: Uint8Array[]): Promise<Uint8Array> {
  return new Uint8Array(await crypto.subtle.digest('SHA-256', frame(parts)));
}

/** The `sha256:`-prefixed form used in the API and the public log. */
export async function framedHex(parts: Uint8Array[]): Promise<string> {
  return 'sha256:' + toHex(await framed(parts));
}

/**
 * The per-trial commitment (D3). The coordinate is inside the hash, which is what makes the
 * reveal prove *this* coordinate pointed at *this* image.
 */
export async function commitment(
  sServer: Uint8Array,
  nonce: Uint8Array,
  coordinate: string,
): Promise<string> {
  return framedHex([sServer, nonce, utf8(coordinate)]);
}
