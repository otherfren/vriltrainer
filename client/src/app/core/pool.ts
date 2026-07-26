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
 * One entry per category, translated at build time like everything else the visitor reads.
 *
 * An unknown identifier falls back to itself rather than to a placeholder. A pool that gained a
 * category before this list caught up must still label its images, and the raw identifier is the
 * truthful thing to show — a "Sonstiges" would quietly merge two categories on screen that the
 * draw treats as separate.
 */
// One entry per category id in `pool/categories.toml`, and nothing else. The four that were
// missing — insect, toy, armour, weapon — printed the raw id at a German visitor after every
// verdict, because the fallback below is deliberately the id rather than a guess. Five that were
// here are gone from the pool (landscape, water, sky, stairs, street) and are dropped with them:
// a label for a category no draw can produce is a string nobody will ever see being wrong.
const CATEGORY_LABELS: Record<string, string> = {
  building: $localize`:@@category.building:Bauwerk`,
  animal: $localize`:@@category.animal:Tier`,
  plant: $localize`:@@category.plant:Pflanze`,
  vehicle: $localize`:@@category.vehicle:Fahrzeug`,
  tool: $localize`:@@category.tool:Werkzeug`,
  food: $localize`:@@category.food:Essen`,
  clothing: $localize`:@@category.clothing:Kleidung`,
  instrument: $localize`:@@category.instrument:Musikinstrument`,
  furniture: $localize`:@@category.furniture:Möbel`,
  machine: $localize`:@@category.machine:Maschine`,
  textile: $localize`:@@category.textile:Stoff`,
  container: $localize`:@@category.container:Behälter`,
  sign: $localize`:@@category.sign:Schild`,
  bridge: $localize`:@@category.bridge:Brücke`,
  door: $localize`:@@category.door:Tür`,
  light: $localize`:@@category.light:Licht`,
  stone: $localize`:@@category.stone:Stein`,
  ornament: $localize`:@@category.ornament:Dekoration`,
  person: $localize`:@@category.person:Mensch`,
  insect: $localize`:@@category.insect:Insekt`,
  toy: $localize`:@@category.toy:Spielzeug`,
  armour: $localize`:@@category.armour:Rüstung`,
  weapon: $localize`:@@category.weapon:Waffe`,
};

export function categoryLabel(id: string): string {
  return CATEGORY_LABELS[id] ?? id;
}
