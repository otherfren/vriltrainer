# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What this is

**vriltrainer is a public remote viewing test.** That is the subject matter, and it should be clear
from the first minute of a session: the user is asked to describe or sense a target that has not
been shown yet.

A trial runs like this. The viewer is given a coordinate - an opaque label with no relationship to
the target. They reveal, and see eight images from eight different categories. They pick the one
they believe is the target. The site tells them whether they hit.

Chance is 12.5%, and the site says so itself. The stats page carries a z-score, so a viewer can see
whether they are actually above chance or just enjoying a streak.

What makes it more than a click game is that **the operator does not have to be trusted.** The
server commits to its half of the randomness before the viewer's half exists, the viewer's client
throws in the other half, and both halves plus the target are published after the pick. Anyone can
recompute the trial at home and confirm the target was fixed in advance. The public log is an
append-only hash chain; an instance refuses to start if the chain does not link.

That is why the two implementations - Rust on the server, TypeScript in the browser - must derive a
trial byte-for-byte identically. Verifiability is the product.

`docs/trial-protocol-decisions.md` (D1-D33) records why the protocol looks the way it does.

## Documents are written in English

Every document in this repository is English: `README.md`, everything under `docs/`, the
specification artifacts, code comments, and commit messages. No exceptions, including documents
that only the operator will read.

**The exception that is not one:** user-facing product copy is content, not documentation.
`vriltrainer.de` ships German strings and `vriltrainer.com` ships English ones — those live in the
client's message catalogues and follow the domain, per D10.

Conversation happens in whatever language the user writes in. That has no bearing on what goes
into a file.

## Git workflow

**Everything happens on `master`. No branches, no worktrees, no pull requests.** The operator does
not want merges to review, and a one-person repository does not earn the ceremony. If a background
session's isolation guard blocks an edit, that is what `.claude/settings.json` sets
`worktree.bgIsolation` to `none` for — do not work around it by creating a worktree.

**Commit and push often** — small, frequent commits rather than one large one at the end of a task.
Commit each coherent step as soon as it works instead of batching up unrelated changes, and push to
`origin` (`git@github.com:otherfren/vriltrainer.git`) right after. No need to ask for permission per
commit; this is the standing instruction for this repo.

## Current state

This is a working application under construction, not a scaffold. `README.md` is the operator-facing
truth for how to run, build, and deploy it; this section covers what an agent needs that the README
does not say.

The shape:

| | |
|---|---|
| `server/` | Rust service — axum + rusqlite (bundled SQLite), edition 2024. The whole API |
| `tools/poolctl/` | Curation tool. Second member of the Cargo workspace |
| `client/` | Angular 19 interface, Karma/Jasmine tests |
| `shared/vectors/derivation.json` | The frozen contract between the two implementations |
| `specs/001-remote-viewing-trainer/` | Spec, plan, contracts, and `tasks.md` (the live worklist) |
| `docs/trial-protocol-decisions.md` | D1–D33 — why the protocol looks the way it does |
| `deploy/` | systemd template unit + nginx config |

`server/src/lib.rs` names its own reading order, and it is the right one: `trial::derive` is the
derivation both implementations must agree on, `log::chain` owns the hashing rule for the public
record, and `db::Db::append_with` is what keeps two processes from forking it. Start there rather
than at `main.rs`.

### Commands

**`cargo` is not on `PATH` in a non-login shell.** It lives at `~/.cargo/bin/cargo`, put there by
`~/.profile`, which a non-interactive `bash -c` does not read. Agent shells are non-login, so either
prefix `export PATH="$HOME/.cargo/bin:$PATH"` or run through `bash -lc`. `node` (22.x) and
`chromium` are in `/usr/bin` and need no such handling.

```bash
cargo test --workspace                 # 277 tests, all passing as of 2026-07-26
cd client && npm run conformance       # 7 derivation vectors — see below
cargo clippy --workspace --all-targets
cargo fmt --all
```

**`npm run conformance` is the most important check in the project.** It runs the TypeScript
derivation against the same vectors the Rust tests use. Rust and TypeScript must compute a trial
byte-for-byte identically, or verification fails in production on honest trials — which is the one
promise the product makes. Run it after touching `server/src/trial/derive.rs`,
`server/src/framing.rs`, or anything under `client/src/app/verify/`. `pretest` and `prebuild` copy
the vectors into the client, so never hand-edit the client's copy.

Regenerating vectors (`cargo run --bin gen_vectors`) changes that contract and retroactively
invalidates every published trial. It is a deliberate act, never a build step. See
`shared/vectors/README.md`.

### Things that are easy to break without noticing

- **The images are compiled into the binary** (D29). `server/build.rs` reads `pool/normalised/`,
  which is generated by `poolctl import` and is not in the repository. A checkout without images
  builds fine and warns; a pool bump is a rebuild, not a file sync.
- **The hash chain gates startup.** Each instance walks the public log at boot and refuses to run if
  it does not link. Do not "fix" that by skipping the check — appending to a record that is already
  wrong is worse than being down.
- **Two processes, one SQLite file.** The append discipline (R9) assumes local-filesystem locking.
- **`Db::reader()` hands back the *writer* on an in-memory database**, and holds it for as long as
  the binding lives. A test that keeps a reader in scope and then reads anything else deadlocks —
  silently, with no output, until the run is killed. Put every reader in its own block. This cost
  two sessions an afternoon between them on 2026-07-26.
- **On a brand-new database file, start the two instances one after another.** The first creates the
  schema and the second dies in `database is locked` while it does. Once the file exists, parallel
  operation is what D24 and R9 are for.
- **`tasks.md` is stale in places.** It carries hand-written unticking notes (e.g. T003 claims axum
  and rusqlite are absent; both have been present for a while). Verify against the code before
  trusting a checkbox, and mark tasks `[X]` as you complete them — `/speckit-implement` reads them.

`.specify/memory/constitution.md` is ratified at **1.0.0** with five principles (Spec Before Code,
Simplicity First, Tests Where They Earn Their Keep, Observable by Default, Explicit Contracts). The
"Constitution Check" gate in `/speckit-plan` reads it and is live.

## The SDD workflow

The Spec Kit install is [v0.14.2](https://github.com/github/spec-kit) (`.specify/` +
`.claude/skills/speckit-*`). Commands are Claude Code skills under `.claude/skills/`. This project
was initialized with `invoke_separator: "-"` (see `.specify/integration.json`), so they are invoked
as `/speckit-plan`, **not** `/speckit.plan`. Any hook or doc that names a command with dots maps to
hyphens.

Two further skills are vendored from [`mattpocock/skills`](https://github.com/mattpocock/skills):
`grill-me` (user-invoked only — a two-line wrapper) delegating to `grilling`, the actual
relentless-interview prompt, which is model-invocable on "grill" phrases.

Feature 001 is well past the planning phases, so the front of this sequence is history rather than
the next thing to run; `/speckit-implement` and `/speckit-converge` are the live ones.

```
/speckit-constitution   # project principles — already ratified at 1.0.0, gates planning
/speckit-specify "…"    # creates specs/NNN-short-name/spec.md + checklists/requirements.md
/speckit-clarify        # optional: max 5 targeted questions, answers written back into spec.md
/speckit-plan           # research.md, data-model.md, contracts/, quickstart.md
/speckit-tasks          # tasks.md, grouped by user story
/speckit-analyze        # optional: cross-artifact consistency check (read-only)
/speckit-implement      # executes tasks.md phase by phase
/speckit-converge       # reconcile codebase vs spec; appends missing work to tasks.md
```

`.specify/workflows/speckit/workflow.yml` chains specify → plan → tasks → implement with approval
gates, driven by the `specify` CLI (installed at `~/.local/bin/specify`).

## Feature context resolution (most common failure mode)

Spec Kit ≥0.14 does **not** infer the active feature from the git branch. Every script resolves the
feature directory via `get_feature_paths()` in `.specify/scripts/bash/common.sh`, in this order:

1. `SPECIFY_FEATURE_DIRECTORY` env var (explicit override; persisted to `.specify/feature.json`)
2. the `feature_directory` key in `.specify/feature.json` (written by `/speckit-specify`)
3. **hard error** — `ERROR: Feature directory not found.`

`SPECIFY_FEATURE` only supplies the `CURRENT_BRANCH` *label*; it does not select the directory. So a
`git checkout` of another feature branch does **not** switch the active feature — `.specify/feature.json`
does. If a script errors out, check that file first. `SPECIFY_INIT_DIR` overrides the repo root
(strict: must contain `.specify/`).

Feature directories are `specs/NNN-short-name/` with sequential 3-digit numbering
(`feature_numbering: "sequential"` in `.specify/init-options.json`). The spec directory name and the
git branch name are deliberately independent. The git extension (which would create branches via a
`before_specify` hook) is **not installed** — there is no `.specify/extensions/` and no
`.specify/extensions.yml`, so all hook-check blocks in the skills skip silently.

## Scripts

All are run from the repo root and all accept `--json` (which the skills parse):

| Script | Purpose |
|---|---|
| `.specify/scripts/bash/create-new-feature.sh --json "<desc>"` | Allocate next `NNN`, create `specs/<dir>/spec.md`, write `feature.json`. Flags: `--short-name`, `--number`, `--timestamp`, `--dry-run`, `--allow-existing-branch`. |
| `.specify/scripts/bash/setup-plan.sh --json` | Copy the resolved plan template into the feature dir; emit `FEATURE_SPEC`/`IMPL_PLAN`/`SPECS_DIR`. |
| `.specify/scripts/bash/setup-tasks.sh --json` | Validate spec.md+plan.md exist, resolve the tasks template, list available design docs. |
| `.specify/scripts/bash/check-prerequisites.sh --json [--require-tasks] [--include-tasks]` | Gate for tasks/implement phases. |
| `.specify/scripts/bash/check-prerequisites.sh --paths-only` | Read-only path dump — the fastest way to see which feature is active. Uses `--no-persist`, so it never dirties `feature.json`. |

`/speckit-specify` creates the feature directory itself (per its skill instructions) rather than
shelling out to `create-new-feature.sh`; the script is the CLI/non-interactive equivalent.

## Template resolution

`resolve_template()` / `resolve_template_content()` in `common.sh` search a four-layer stack, highest
priority first:

1. `.specify/templates/overrides/<name>.md` — **project customization goes here**
2. `.specify/presets/<id>/templates/` (ordered by `priority` in `.specify/presets/.registry`; supports
   `replace` / `prepend` / `append` / `wrap` composition strategies, `wrap` requiring a
   `{CORE_TEMPLATE}` placeholder)
3. `.specify/extensions/<id>/templates/`
4. `.specify/templates/<name>.md` — core, shipped

Do **not** edit the core templates in `.specify/templates/` directly: their SHA-256 hashes are
recorded in `.specify/integrations/claude.manifest.json` and a Spec Kit upgrade/reinstall overwrites
them. The same applies to `.claude/skills/speckit-*/SKILL.md`. Add an override instead.

## Artifact conventions

Per feature directory: `spec.md`, `plan.md`, `tasks.md`, `research.md`, `data-model.md`,
`quickstart.md`, `contracts/`, `checklists/`.

- **spec.md** — WHAT/WHY only, no tech stack or APIs. At most 3 `[NEEDS CLARIFICATION]` markers;
  everything else becomes a documented assumption. Success criteria must be measurable *and*
  technology-agnostic ("results in under 1 second", not "p95 < 200ms").
- **tasks.md** — format `[ID] [P?] [Story] Description` with exact file paths. `T001`-style IDs,
  `[P]` = parallelizable (different files, no dependency), `[US1]` = owning user story. Tasks are
  grouped into Phase 1 Setup → Phase 2 Foundational (blocks everything) → one phase per user story in
  priority order → Polish. Each user story must stay independently implementable and testable.
  Mark completed tasks `[X]` as you go — `/speckit-implement` relies on this.
- **Tests are opt-in.** The task template omits test tasks unless the spec explicitly asks for them.
  When they are included, they must be written and failing before the implementation task runs.
- `/speckit-implement` halts on a failed sequential task; for `[P]` tasks it continues and reports
  the failures.
