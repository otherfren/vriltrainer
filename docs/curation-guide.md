# Curating the image pool

The pool is the one part of this project no software can produce. It decides whether the trials
are worth anything, and it blocks everything else until it exists. This guide is written to be
followed by someone other than the operator.

## What you are doing, in one sentence

Find images, record where they came from and under what licence, assign a category, run them
through the tool.

Two files describe the pool, and you edit both by hand:

- `pool/categories.toml` — every category an image may use, `id` plus its German and English name.
  A category not listed there is refused as a typo.
- `pool/images.toml` — the images. Paths are relative to that file, under `base` (which is
  `pool/images/`), and **the folder layout beneath it is yours**. Organising by category is the
  obvious arrangement and what the seeded file does, but nothing enforces it: identity is the hash
  of the normalised bytes and the category is what the entry says, so renaming a folder changes
  nothing and changing a `category` changes every future draw.

`poolctl` writes its own output to `pool/normalised/` and `pool/catalogue.json`. Both are build
artefacts, both are gitignored, and `import` recreates them from the sources.

```bash
poolctl import                # takes everything images.toml names into the catalogue
poolctl check                 # complains while it is still cheap
poolctl build --version 1     # normalises, hashes, writes shared/pool/v1.json
```

`import` is idempotent — an image already held is skipped — so the normal way to grow the pool is
to append entries and run it again.

## Where images may come from

**CC0 and public domain only.** No CC-BY, no "looks free" images, no image search.

| Source | What to check |
|---|---|
| Wikimedia Commons | The licence is on the file page. "PD-old" and "CC0" are fine, "CC-BY-SA" is not |
| Unsplash | Its own licence, usable for this purpose; record the source anyway |
| Pexels | As Unsplash |
| openverse | Set the filter to CC0, not to "all" |

**Why not CC-BY:** the attribution would have to appear visibly with the image. One image out of
eight carrying a line of text is distinguishable from the other seven, and therefore identifiable
as the target without anything paranormal being involved.

**Why so strict at all:** the site runs on a `.de` domain and looks commercial, which makes the
operator convenient to reach for a legal notice. Five minutes of licence checking is cheaper than
a letter.

## What makes a usable target

An image works if someone can form an impression with their eyes closed and then recognise it.

**Take:** clear subjects, strong shapes, an unambiguous thing in the frame. A lighthouse. A horse.
A bridge at night. A plate of cherries.

**Leave:** diffuse textures, fog, blur, "aesthetic" shots without a subject. Someone whose
impression is "something grey" cannot choose between three grey images.

## What never enters the pool

- **Legible text in the image.** Signs, logos, captions. Bad twice over: text marks one image
  among eight, and on a bilingual site a German word also sits wrongly on the English domain.
- **Recognisable faces.** Personality rights, and you do not want the conversation.
- **Anything with an arguable licence.** If you have to make a case, the answer is no.
- **Images you already have.** `poolctl check` catches this by hash, but looking first saves time.

## Categories

Every image gets exactly one. There are 16 to 24 of them, and each trial draws **eight different
ones** — which is why a user never sees two images of the same kind side by side.

**Consistency matters more than precision.** A category is a drawing bucket, not a taxonomy. If an
image fits two, pick the one you would look for it in later, and stay with that choice. A
waterfall is either always `landscape` or always `water` — just not one and then the other.

Rule of thumb for how finely to cut: two images in the same category should still be clearly
different from each other. When `landscape` holds nothing but mountains, it is time to split off
`mountains`.

## Variety inside a category

This is the part categories do **not** solve and only you can.

The draw guarantees that a set consists of eight different categories. It can do nothing about
your twenty landscape images all being coastlines at sunset. Two of them will then never appear
together — but across a hundred trials it becomes monotonous, and people stop.

When topping up a category, look at the twenty already in it before adding the twenty-first.

## Record provenance immediately

While adding, not later. A lost source URL cannot be recovered, and it is the only thing that
substantiates the licence. `poolctl check` refuses images with no source and no licence — not out
of pedantry, but because that is the last point at which it is still cheap.

## When to cut a new version

Pool versions are immutable. Every trial records which version it ran under and stays verifiable
against it forever. Therefore:

- Collect at leisure, then cut in one go.
- A cut makes sense at roughly fifty new images, or when a category is added.
- After cutting, touch nothing in that version. Not even something small — the order in the
  manifest determines which image holds which index, and the indices determine every future draw.
  A quick re-sort retroactively changes what trials should have drawn.

## The launch pool

A hundred and sixty images across 16 to 24 categories, so roughly eight to ten per category. That is
what `poolctl check` reports against; five hundred is v2, cut while the site is already live.

D5 originally said five hundred at launch on an argument that turned out to be wrong — pool size
does not appear anywhere in the target's distribution, so a lookup table over past image sets buys
an attacker nothing. What P actually controls is how often a player meets the same picture twice:
about `8T/P` times over `T` trials, five times at P = 160 over a hundred trials. That is a boredom
constraint, and it degrades gracefully instead of falling off a cliff, which is why launching at 160
is a delay worth not taking.

Even so it is several evenings of work. There is no shortcut: the pipeline can be automated, the
selection cannot. Start early — until the pool exists, nothing is playable.
