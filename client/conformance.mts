import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { derive, membersOf } from './src/app/verify/derive.ts';
import { commitment, fromHex } from './src/app/verify/framing.ts';

const here = dirname(fileURLToPath(import.meta.url));
const cases = JSON.parse(
  readFileSync(join(here, '..', 'shared', 'vectors', 'derivation.json'), 'utf8'),
);
let failed = 0;
for (const c of cases) {
  const members = membersOf(c.categories, c.images);
  const got = await derive(fromHex(c.s_server), fromHex(c.s_client), members);
  const eq = (a: unknown, b: unknown) => JSON.stringify(a) === JSON.stringify(b);
  const ok =
    eq(got.chosenCategories, c.expect.chosen_categories) &&
    eq(got.selectedImages, c.expect.selected_images) &&
    got.targetSlot === c.expect.target_slot &&
    eq(got.displayOrder, c.expect.display_order);
  console.log(`${ok ? 'ok  ' : 'FAIL'}  ${c.name}`);
  if (!ok) {
    failed++;
    console.log('   rust:', JSON.stringify(c.expect));
    console.log('   ts  :', JSON.stringify(got));
  }
}
const a = await commitment(new Uint8Array([1, 2]), new Uint8Array([3]), 'x');
const b = await commitment(new Uint8Array([1]), new Uint8Array([2, 3]), 'x');
console.log(`${a !== b ? 'ok  ' : 'FAIL'}  commitment field boundaries`);
if (a === b) failed++;
console.log(failed === 0 ? `\nALL ${cases.length} CASES AGREE` : `\n${failed} FAILURES`);
process.exit(failed === 0 ? 0 : 1);
