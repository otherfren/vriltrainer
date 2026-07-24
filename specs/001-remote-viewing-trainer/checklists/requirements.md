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

- [ ] No [NEEDS CLARIFICATION] markers remain
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

**One item fails: two [NEEDS CLARIFICATION] markers remain**, both in Edge Cases. Neither has a
defensible default, which is why they were left rather than guessed:

1. **Abandoned trials.** A trial revealed but never answered has no settled treatment. This is
   not cosmetic: if a user can silently drop trials that feel wrong, they select their own
   sample, and every figure on the statistics page inherits the bias. It interacts directly with
   FR-013 (trials recorded at creation), FR-018 (aggregate over all trials) and SC-005 (the
   aggregate must not be inflated).
2. **Name removal route.** The right to erasure is satisfied structurally by FR-023, but whether
   removal is self-service or handled on request is unsettled and changes both the interface and
   the operational burden.

**Deliberate wording choices, recorded so they are not mistaken for vagueness:**

- Mechanism is referenced, never specified. Terms such as "evidence the user can check" stand in
  for the construction fixed in `docs/trial-protocol-decisions.md` D3, which belongs in the plan.
- FR-024 states the ranking property rather than the ranking rule; SC-009 makes it testable
  without naming the statistic used.
- "Append-only record" and "head value" are domain vocabulary rather than implementation detail —
  they are what the product promises, and no plainer phrasing preserves the meaning.
