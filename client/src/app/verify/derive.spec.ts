/**
 * Conformance against the same fixtures the Rust server is held to.
 *
 * A failure here means the two implementations have drifted, which in production shows up as
 * verification failing on honest trials — expensive to diagnose, and fatal to the credibility the
 * whole design exists to build.
 */
import vectors from './derivation.vectors.json';
import { derive, membersOf, SET_SIZE } from './derive';
import { commitment, fromHex } from './framing';

interface Case {
  name: string;
  s_server: string;
  s_client: string;
  categories: string[];
  images: { id: string; category: string }[];
  expect: {
    chosen_categories: number[];
    selected_images: number[];
    target_slot: number;
    display_order: number[];
  };
}

const cases = vectors as Case[];

describe('derivation conformance', () => {
  it('has fixtures to check', () => {
    expect(cases.length).toBeGreaterThan(0);
  });

  cases.forEach((c) => {
    it(`reproduces ${c.name}`, async () => {
      const members = membersOf(c.categories, c.images);
      const got = await derive(fromHex(c.s_server), fromHex(c.s_client), members);
      expect(got.chosenCategories).toEqual(c.expect.chosen_categories);
      expect(got.selectedImages).toEqual(c.expect.selected_images);
      expect(got.targetSlot).toEqual(c.expect.target_slot);
      expect(got.displayOrder).toEqual(c.expect.display_order);
    });
  });

  // Guards the fixture set itself: with only even categories, a size-proportional target draw
  // would pass unnoticed, and that is precisely the bias D22 exists to prevent.
  it('still includes a lopsided pool', () => {
    const lopsided = cases.some((c) => {
      const sizes = membersOf(c.categories, c.images).map((m) => m.length);
      const min = Math.min(...sizes);
      return min > 0 && Math.max(...sizes) >= min * 10;
    });
    expect(lopsided).toBeTrue();
  });

  it('produces a permutation and an in-range target', async () => {
    const c = cases[0];
    const got = await derive(
      fromHex(c.s_server),
      fromHex(c.s_client),
      membersOf(c.categories, c.images),
    );
    expect([...got.displayOrder].sort((a, b) => a - b)).toEqual([0, 1, 2, 3, 4, 5, 6, 7]);
    expect(got.targetSlot).toBeLessThan(SET_SIZE);
  });
});

describe('commitment', () => {
  it('binds the coordinate', async () => {
    const s = new Uint8Array([1, 2, 3]);
    const n = new Uint8Array([4, 5, 6]);
    expect(await commitment(s, n, '4821-9037')).not.toEqual(await commitment(s, n, '0000-0000'));
  });

  it('cannot have its field boundaries shifted', async () => {
    const a = await commitment(new Uint8Array([1, 2]), new Uint8Array([3]), 'x');
    const b = await commitment(new Uint8Array([1]), new Uint8Array([2, 3]), 'x');
    expect(a).not.toEqual(b);
  });
});
