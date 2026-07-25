export interface NameCheck {
  ok: boolean;
  /** Why it was refused, in the interface's voice. Absent when `ok`. */
  message?: string;
}

export const NAME_MIN = 3;
export const NAME_MAX = 20;

/**
 * Leet is folded before the word lists are applied, so `h1tl3r` and `f0tze` are caught by the
 * same entry as the plain spelling. Only substitutions that read as the letter — nothing so
 * aggressive that ordinary names start matching.
 */
function fold(name: string): string {
  return name
    .toLowerCase()
    .replace(/[0]/g, 'o')
    .replace(/[1|!]/g, 'i')
    .replace(/[3]/g, 'e')
    .replace(/[4@]/g, 'a')
    .replace(/[5$]/g, 's')
    .replace(/[7]/g, 't')
    .replace(/[8]/g, 'b')
    .replace(/[^a-zäöüß]/g, '');
}

/** Names the interface uses for itself, which nobody else gets to wear. */
const RESERVED =
  /^(admin|administrator|mod|moderator|system|root|server|support|team|staff|vriltrainer|vril|anonym|anonymous|null|undefined|nan)$/i;

/**
 * Hate and extremist terms. This is a public leaderboard on a site whose subject matter is
 * adjacent to the Vril and Reichsflugscheibe mythology, so this list is not decoration — the
 * board would attract exactly this without it.
 */
const HATE = [
  /hitler/,
  /siegheil/,
  /heilhitler/,
  /nsdap/,
  /waffenss/,
  /schutzstaffel/,
  /hakenkreuz/,
  /swastika/,
  /judensau/,
  /untermensch/,
  /rassenschande/,
  /whitepower/,
  /kukluxklan/,
  /nigg(a|er)/,
  /faggot/,
  /kanacke/,
  /zigeuner/,
];

/** Ordinary vulgarity. Not a moral position, just not a name on a scoreboard. */
const VULGAR = [
  /fotze/,
  /wichser/,
  /hurensohn/,
  /arschloch/,
  /schwanzlutscher/,
  /bastard/,
  /cunt/,
  /fuck/,
  /shit/,
  /penis/,
  /vagina/,
];

/** Digits and separators only, e.g. `1488`, so numeric codes cannot slip past the letter rule. */
const NUMERIC_CODE = /^[\d\s_-]+$/;

/**
 * Checks a display name.
 *
 * This runs in the browser for the sake of telling somebody what is wrong while they type. It
 * is not the enforcement point: the server has to apply the same rules on `POST /api/account`,
 * because anything checked only in the client is not checked at all.
 */
export function checkDisplayName(raw: string): NameCheck {
  const name = normaliseDisplayName(raw);

  if (name.length === 0) return { ok: false, message: 'Bitte gib einen Namen ein.' };
  if (name.length < NAME_MIN)
    return { ok: false, message: `Mindestens ${NAME_MIN} Zeichen.` };
  if (name.length > NAME_MAX) return { ok: false, message: `Höchstens ${NAME_MAX} Zeichen.` };

  if (!/^[\p{L}\p{N} _-]+$/u.test(name))
    return { ok: false, message: 'Nur Buchstaben, Ziffern, Leerzeichen, Bindestrich, Unterstrich.' };

  if (/^[ _-]|[ _-]$/.test(name))
    return { ok: false, message: 'Nicht mit Leerzeichen, - oder _ anfangen oder aufhören.' };

  if (NUMERIC_CODE.test(name)) return { ok: false, message: 'Nicht nur Ziffern.' };

  if (!/\p{L}[\s\S]*\p{L}/u.test(name))
    return { ok: false, message: 'Mindestens zwei Buchstaben.' };

  if (/(.)\1\1\1/u.test(name))
    return { ok: false, message: 'Nicht viermal dasselbe Zeichen hintereinander.' };

  if (RESERVED.test(name))
    return { ok: false, message: 'Der Name ist für die Seite selbst reserviert.' };

  if (/(https?:|www\.|\.(de|com|net|org|io)\b)/i.test(name))
    return { ok: false, message: 'Keine Adressen im Namen.' };

  const folded = fold(name);
  if (HATE.some((r) => r.test(folded)))
    return { ok: false, message: 'Such dir etwas anderes aus.' };
  if (VULGAR.some((r) => r.test(folded)))
    return { ok: false, message: 'Der Name steht auf einer öffentlichen Rangliste. Nicht dieser.' };

  return { ok: true };
}

/** Trimmed, with runs of whitespace collapsed. What gets stored and displayed. */
export function normaliseDisplayName(raw: string): string {
  return raw.replace(/\s+/g, ' ').trim();
}
