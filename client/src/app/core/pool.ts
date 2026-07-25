/**
 * Where a pool image lives, and what its category is called in German.
 *
 * Neither is in the manifest, and both are deliberately absent from it. The manifest carries
 * identifiers and categories and nothing else, because anything per-image that could differ
 * between the target and its seven decoys — a caption, a credit, a size — is a sensory channel
 * and the classic way a forced-choice experiment produces a false positive (D5).
 */

/**
 * The site-relative address of a pool image.
 *
 * `img_<hex>` identifiers are the hash of the normalised bytes, and `poolctl` writes those bytes
 * to `pool/normalised/<id>.png`. The deployment copies that directory into the public root as
 * `pool/`, alongside the built bundle — see `client/README.md`. It is a static path rather than
 * an API endpoint on purpose: the bytes are immutable under their own hash, so they cache
 * forever and never need the server to think about them.
 */
export function imageSrc(id: string): string {
  return `pool/${id}.png`;
}

/**
 * The German name of a pool category.
 *
 * Product copy, so it lives in the client and follows the domain — the `.com` build carries the
 * English list. The identifier is what goes into the derivation and into the manifest hash; this
 * is only what a visitor reads.
 *
 * An unknown identifier falls back to itself rather than to a placeholder. A pool that gained a
 * category before this list caught up must still label its images, and the raw identifier is the
 * truthful thing to show — a "Sonstiges" would quietly merge two categories on screen that the
 * draw treats as separate.
 */
const CATEGORY_DE: Record<string, string> = {
  landscape: 'Landschaft',
  building: 'Bauwerk',
  animal: 'Tier',
  plant: 'Pflanze',
  vehicle: 'Fahrzeug',
  tool: 'Werkzeug',
  food: 'Essen',
  clothing: 'Kleidung',
  instrument: 'Instrument',
  furniture: 'Möbel',
  water: 'Wasser',
  sky: 'Himmel',
  machine: 'Maschine',
  textile: 'Textil',
  container: 'Behälter',
  sign: 'Schild',
  stairs: 'Treppe',
  bridge: 'Brücke',
  door: 'Tür',
  light: 'Licht',
  stone: 'Stein',
  ornament: 'Ornament',
  person: 'Mensch',
  street: 'Straße',
};

export function categoryLabel(id: string): string {
  return CATEGORY_DE[id] ?? id;
}
