# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Git workflow

**Commit and push often** — small, frequent commits rather than one large one at the end of a task.
Commit each coherent step as soon as it works instead of batching up unrelated changes, and push to
`origin` (`git@github.com:otherfren/vriltrainer.git`) right after. Work happens directly on `master`.
No need to ask for permission per commit; this is the standing instruction for this repo.

## Current state

This repository is a **greenfield Spec-Driven Development (SDD) scaffold**, not yet an application.
It contains `README.md`, a GitHub [Spec Kit](https://github.com/github/spec-kit) v0.14.2 installation
(`.specify/` + `.claude/skills/speckit-*`), and two skills vendored from
[`mattpocock/skills`](https://github.com/mattpocock/skills): `grill-me` (user-invoked only — a
two-line wrapper) delegating to `grilling` (the actual relentless-interview prompt, model-invocable
on "grill" phrases). There is **no source code, no build system,
no test runner, and no dependency manifest yet** — so there are no build/lint/test commands to run.
The tech stack gets chosen and recorded in the first `/speckit-plan` (its "Technical Context" and
"Structure Decision" sections), and the actual project scaffolding is created by the first
`/speckit-implement` run (Phase 1: Setup tasks). Once that happens, this file should be updated with
the real build/test commands.

`.specify/memory/constitution.md` is still the **unfilled template** (all `[PLACEHOLDER]` values).
`/speckit-plan` has a hard "Constitution Check" gate that reads it, so run `/speckit-constitution`
before serious planning, or the gate is meaningless.

## The SDD workflow

Commands are Claude Code skills under `.claude/skills/`. This project was initialized with
`invoke_separator: "-"` (see `.specify/integration.json`), so they are invoked as `/speckit-plan`,
**not** `/speckit.plan`. Any hook or doc that names a command with dots maps to hyphens.

Typical order:

```
/speckit-constitution   # fill in project principles (do this first — it gates planning)
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
