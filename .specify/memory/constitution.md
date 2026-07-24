<!--
SYNC IMPACT REPORT
Version change: unversioned template → 1.0.0 (initial ratification)

Principles defined (all new, none renamed or removed):
  I.   Spec Before Code
  II.  Simplicity First
  III. Tests Where They Earn Their Keep
  IV.  Observable by Default
  V.   Explicit Contracts

Added sections: Technology & Platform Constraints; Development Workflow
Removed sections: none (template placeholder sections were named, not dropped)

Template / artifact consistency:
  ✅ .specify/templates/plan-template.md   — no change required; its "Constitution Check"
                                             section resolves gates from this file at plan time
  ✅ .specify/templates/spec-template.md   — aligned; mandatory sections already match
                                             Principle I and the Success Criteria rules
  ✅ .specify/templates/tasks-template.md  — aligned; its opt-in treatment of test tasks
                                             matches Principle III (would have conflicted
                                             had TDD been made mandatory)
  ✅ .claude/skills/speckit-*/SKILL.md     — generic hyphen-form command references, no
                                             stale agent-specific naming
  ✅ CLAUDE.md                             — consistent; its Git workflow section matches
                                             the commit cadence required below
  ✅ README.md                             — contains no principle references to update

Deferred TODOs (see Technology & Platform Constraints):
  TODO(PRODUCT_DOMAIN) — what vriltrainer does, and for whom, is not recorded anywhere
                         in the repository yet
  TODO(TECH_STACK)     — language, framework, storage and deployment target are undecided
-->

# vriltrainer Constitution

## Core Principles

### I. Spec Before Code

Every behavioural change MUST originate in a feature specification under `specs/<NNN-name>/`.
Implementation MUST NOT begin before `spec.md` and `plan.md` exist and `tasks.md` has been
generated. Exempt: typo fixes, formatting, and dependency bumps that change no behaviour.

Rationale: the entire toolchain in this repository assumes the spec is the source of truth.
Code written ahead of its spec makes every downstream artifact — plan, tasks, contracts — a
retroactive fiction, and the reasoning behind a decision becomes unrecoverable.

### II. Simplicity First

The simplest implementation that satisfies the spec MUST be chosen. Every added abstraction
layer, service boundary, or third-party dependency MUST be recorded in the plan's Complexity
Tracking table together with the simpler alternative that was rejected and why.

Rationale: this is a solo project. Every abstraction is maintenance burden paid by exactly one
person, and unjustified structure is the cheapest thing to add and the most expensive to remove.

### III. Tests Where They Earn Their Keep

Tests are SHOULD, not MUST, with two exceptions that are non-negotiable:

- Every fixed bug MUST gain a regression test that fails before the fix and passes after it.
- Every released contract (HTTP endpoint, persisted schema) MUST have at least one test that
  exercises it.

Test tasks appear in `tasks.md` only when the specification asks for them. Where tests are
written, they MUST fail before the corresponding implementation task is run.

Rationale: blanket TDD on a greenfield solo project taxes exploration without a proportional
payoff. A ratchet at the two places where regressions actually reach users — fixed bugs and
published surfaces — keeps the guarantees that matter without the ceremony.

### IV. Observable by Default

Log output MUST be structured and machine-parseable. Every inbound request MUST carry a
correlation identifier through to its log lines. Every error path MUST record enough context to
diagnose the failure without reproducing it. A caught exception MUST be either handled
meaningfully or logged — never silently swallowed.

Rationale: a solo operator debugging production has no colleague to ask and no second pair of
eyes. The logs are the entire investigation.

### V. Explicit Contracts

The externally reachable surface — HTTP endpoints, persisted data schemas, and any published
event formats — MUST be documented in the owning feature's `contracts/` directory. A breaking
change to a released contract MUST increment the MAJOR version and be recorded in the plan.

Rationale: a service has consumers, including its own frontend. Undocumented surfaces break
silently and are discovered by users rather than by the author.

## Technology & Platform Constraints

vriltrainer is a web application/service.

- TODO(PRODUCT_DOMAIN): what vriltrainer does and for whom is not yet recorded anywhere in this
  repository. The first `/speckit-specify` MUST establish it; this section MUST then be amended.
- TODO(TECH_STACK): language, framework, storage, and deployment target are undecided. They will
  be fixed in the first `/speckit-plan` "Technical Context" section and MUST be back-ported here
  as a MINOR amendment, so that later plans are constrained by the same choices.
- Configuration MUST come from the environment. Secrets MUST NOT appear in the repository, nor
  in spec, plan, research, or task artifacts.
- Every new runtime dependency MUST be justified in the plan that introduces it (see Principle II).

## Development Workflow

- Phase order: `/speckit-constitution` → `/speckit-specify` → optional `/speckit-clarify` →
  `/speckit-plan` → `/speckit-tasks` → `/speckit-implement`. Phases MUST NOT be skipped forward;
  a missing prerequisite is an error to fix, not a gate to bypass.
- The Constitution Check in `plan.md` MUST be evaluated against this document. An unjustified
  violation blocks the plan; a justified one belongs in the Complexity Tracking table.
- Completed tasks MUST be marked `[X]` in `tasks.md` as they land, not in a batch afterwards.
- Commits MUST be small and frequent, pushed to `origin` after each coherent step. Work happens
  directly on `master`.
- No pull-request approval is required, as this is a solo project. The phase gates above ARE the
  review, and MUST NOT be relaxed to compensate for the absence of a second reviewer.

## Governance

This constitution supersedes ad-hoc practice. Where a habit and this document conflict, either
the practice changes or the constitution is amended — it is never silently ignored.

Amendments MUST be made through `/speckit-constitution`, which versions this document under
semantic versioning: MAJOR for a principle removal or an incompatible redefinition, MINOR for a
new or materially expanded principle or section, PATCH for clarifications and wording. Every
amendment MUST refresh the Sync Impact Report above and re-check the dependent templates listed
there.

Compliance is reviewed at two points: the Constitution Check gate during `/speckit-plan`, and
`/speckit-analyze` when it is run. Runtime guidance for coding agents lives in `CLAUDE.md`;
that file MUST NOT contradict this document, and loses to it where it does.

**Version**: 1.0.0 | **Ratified**: 2026-07-25 | **Last Amended**: 2026-07-25
