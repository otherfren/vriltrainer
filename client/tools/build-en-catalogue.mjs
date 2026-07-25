/**
 * Builds `src/locale/messages.en.xlf` from the extracted `messages.xlf` plus `en.json`.
 *
 * Angular's extractor owns `messages.xlf`: it is regenerated whenever a string changes, so
 * hand-editing it is pointless. The English text lives in `en.json`, keyed by message id, and this
 * script joins the two.
 *
 * The join is the point. A translation whose id no longer exists is reported rather than silently
 * dropped, and a message with no translation is reported rather than shipped in German — which is
 * the failure this whole exercise exists to fix, so it must not be the quiet default.
 *
 *   node tools/build-en-catalogue.mjs [--check]
 *
 * `--check` writes nothing and exits non-zero if the two disagree. That is the form for CI.
 *
 * **Placeholders are written `{ID}` in `en.json`, never as XML.** A message like
 * `Mindestens <x id="min" .../> Zeichen.` is translated `At least {min} characters.`, and this
 * script splices the source's own `<x/>` element back in. Two reasons: an English string that
 * mistypes a placeholder id compiles and then renders a sentence with a hole in it, and nobody
 * should have to write XLIFF by hand to change a sentence.
 */
import { readFileSync, writeFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';

const here = dirname(fileURLToPath(import.meta.url));
const localeDir = join(here, '..', 'src', 'locale');
const check = process.argv.includes('--check');

const source = readFileSync(join(localeDir, 'messages.xlf'), 'utf8');
const english = JSON.parse(readFileSync(join(localeDir, 'en.json'), 'utf8'));

const units = [...source.matchAll(/<trans-unit id="([^"]+)"[^>]*>([\s\S]*?)<\/trans-unit>/g)].map(
  ([, id, body]) => ({
    id,
    body,
    source: body.match(/<source>([\s\S]*?)<\/source>/)?.[1] ?? '',
  }),
);

/** Every `<x id="…"/>` in a source body, by id, kept exactly as the extractor wrote it. */
function placeholders(text) {
  const found = new Map();
  for (const [tag, id] of text.matchAll(/<x id="([^"]+)"[^>]*\/>/g)) found.set(id, tag);
  return found;
}

const ids = new Set(units.map((u) => u.id));
const problems = [];

// Keys beginning with `_` are notes to whoever edits the file, not messages.
for (const id of Object.keys(english)) {
  if (id.startsWith('_')) continue;
  if (!ids.has(id)) problems.push(`${id}: Übersetzung ohne Meldung im Quellkatalog`);
}

const rendered = new Map();
for (const unit of units) {
  const target = english[unit.id];
  if (target === undefined) {
    problems.push(`${unit.id}: keine englische Fassung`);
    continue;
  }

  // An ICU message carries its own braces — `{VAR_PLURAL, plural, =1 {…} other {…}}` — and the
  // inner forms are text, not placeholders. Taking it verbatim is the only correct reading; the
  // brace convention below would mangle it.
  if (/\{VAR_(PLURAL|SELECT)/.test(unit.source)) {
    rendered.set(unit.id, target);
    continue;
  }

  const wanted = placeholders(unit.source);
  const used = new Set([...target.matchAll(/\{([A-Za-z0-9_]+)\}/g)].map(([, k]) => k));

  for (const k of used) {
    if (!wanted.has(k)) problems.push(`${unit.id}: Platzhalter {${k}} gibt es im Original nicht`);
  }
  for (const k of wanted.keys()) {
    if (!used.has(k)) problems.push(`${unit.id}: Platzhalter {${k}} fehlt in der Übersetzung`);
  }

  rendered.set(
    unit.id,
    target.replace(/\{([A-Za-z0-9_]+)\}/g, (whole, k) => wanted.get(k) ?? whole),
  );
}

if (problems.length > 0) {
  console.error(`\n${problems.length} Problem(e):`);
  for (const p of problems) console.error(`  ${p}`);
}

if (check) {
  if (problems.length === 0) console.log(`ok — ${units.length} Meldungen, alle übersetzt`);
  process.exit(problems.length === 0 ? 0 : 1);
}

let out = source.replace(
  /<file([^>]*?)source-language="de"/,
  '<file$1source-language="de" target-language="en"',
);

for (const unit of units) {
  const target = rendered.get(unit.id);
  if (target === undefined) continue;
  out = out.replace(
    unit.body,
    `${unit.body.replace(/\s*$/, '')}\n        <target>${target}</target>\n      `,
  );
}

writeFileSync(join(localeDir, 'messages.en.xlf'), out);
console.log(`messages.en.xlf: ${rendered.size}/${units.length} Meldungen`);
if (problems.length > 0) process.exitCode = 1;
