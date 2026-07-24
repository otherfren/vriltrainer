# Specification Quality Checklist: Remote Viewing Trainer

**Purpose**: Validate specification completeness and quality before proceeding to planning
**Created**: 2026-07-25
**Feature**: [spec.md](../spec.md)

## Content Quality

- [X] No implementation details (languages, frameworks, APIs)
- [X] Focused on user value and business needs
- [X] Written for non-technical stakeholders
- [X] All mandatory sections completed

## Requirement Completeness

- [X] No [NEEDS CLARIFICATION] markers remain
- [X] Requirements are testable and unambiguous
- [X] Success criteria are measurable
- [X] Success criteria are technology-agnostic (no implementation details)
- [X] All acceptance scenarios are defined
- [X] Edge cases are identified
- [X] Scope is clearly bounded
- [X] Dependencies and assumptions identified

## Feature Readiness

- [X] All functional requirements have clear acceptance criteria
- [X] User scenarios cover primary flows
- [X] Feature meets measurable outcomes defined in Success Criteria
- [X] No implementation details leak into specification

## Notes

- Items marked incomplete require spec updates before `/speckit-clarify` or `/speckit-plan`

### Validation pass 1 — 2026-07-25

One item failed: two `[NEEDS CLARIFICATION]` markers in Edge Cases, covering the treatment of
abandoned trials and the route for removing a name. Neither had a defensible default.

### Validation pass 2 — 2026-07-25 — all items pass

Both markers resolved and folded into requirements.

**Abandoned trials** are marked and retained, excluded from hit rate, completed-trial total and
the statistics threshold, but present in the public record and counted in a published
abandonment rate (FR-014, FR-016, FR-021, FR-027, SC-012). The reasoning is worth preserving:
this does not prevent a user from selecting their own sample by dropping trials that feel wrong.
It makes that selection **measurable**, which is the honest option — and it is only possible
because FR-013 already writes a trial to the record before its outcome exists.

**Name removal is self-service**, proved by possession of the access link (FR-035, FR-036).
Handling it on request was not merely less convenient but unworkable: no email is on file, so
there is no way to authenticate a requester. The access link is the only proof of ownership that
exists.

Renumbering note: six requirements were added inside existing groups, so FR numbers after FR-013
shifted. Cross-references in Assumptions were updated to match (FR-028 for the ranking rule,
FR-020 for the aggregate).

**Deliberate wording choices, recorded so they are not mistaken for vagueness:**

- Mechanism is referenced, never specified. Terms such as "evidence the user can check" stand in
  for the construction fixed in `docs/trial-protocol-decisions.md` D3, which belongs in the plan.
- FR-028 states the ranking property rather than the ranking rule; SC-009 makes it testable
  without naming the statistic used.
- "Append-only record" and "head value" are domain vocabulary rather than implementation detail —
  they are what the product promises, and no plainer phrasing preserves the meaning.

**Carried into planning as an open parameter**, not a specification gap: the period after which
an unanswered trial counts as abandoned. It is recorded in Assumptions and must be long enough
that a slow viewing session is not cut off mid-impression.

### Amendment — 2026-07-25, after D16

The trial's working state was moved into a server-encrypted token held by the client, which
removes `s_server` from the database entirely. Two spec consequences:

- **FR-014** no longer refers to any elapsed time. A trial is abandoned by not being completed,
  so the Assumptions entry naming an abandonment period was removed rather than given a value.
- **FR-037** was added: at most one answer per trial. Without it a client could resubmit the
  same token with a different image until it hit, then file a clean run — the token is
  self-contained, so nothing else prevents replay. This is what forces a row to be written at
  trial creation, which in turn is what keeps FR-027 and SC-012 implementable.

FR-037 carries the next free number rather than sitting in numeric position, so that references
elsewhere keep pointing at the same requirements. Numbers here identify, they do not order.

Still passing 16/16; no new clarification markers.
