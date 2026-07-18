# RIPR-SPEC-0127: Active-goal authority audit

Status: accepted

This specification defines the audit-only prerequisite for removing singleton
execution authority from `.ripr/goals/active.toml`. It implements issue #1697
and does not perform the migration owned by #1701.

## Problem

The manifest is consumed by code, checks, fixtures, policy, documentation,
agent guidance, historical handoffs, and issue contracts. Moving or weakening
the file without classifying every consumer can leave hidden selection or
mutation authority behind.

## Behavior

`cargo xtask active-goal-authority-audit` scans repository source text for the
versioned singleton markers, joins every discovered path to the reviewed
consumer inventory, and writes deterministic JSON and Markdown under
`target/ripr/reports/`.

Every consumer row records its current behavior, fields consumed, authority
effect, target classification, owner, dependent issue, compatibility period,
and positive and negative proof. The closed classifications are those defined
by #1697.

Unclassified discoveries, equally specific contradictory rules, and
`unknown_needs_decision` rows block `migration_ready`. Unused inventory rules
remain visible but do not by themselves strengthen or weaken readiness. The
report still emits when blockers exist so the exact repair route remains
inspectable.

## Required invariants

- Equivalent repository inputs produce byte-equivalent semantic rows and digest.
- Absolute checkout spelling and generation time are absent from semantic
  identity.
- A historical reference remains readable and has no current authority.
- A legacy `ready`, `done`, `blocked`, branch, or lane value cannot authorize
  current work.
- Removing only `active.toml` does not hide a remaining singleton reader.
- The command reads repository inputs and writes only ignored reports.
- Captured issue contracts are inventory evidence, not live GitHub state.

## Required Evidence

The proof includes the current-repository completeness check, deterministic
repeat check, a hidden-reader fixture with no `active.toml`, a legacy-ready
non-authorization fixture, and a historical-reference non-blocker.

## Non-Goals

- Moving, deleting, or reinterpreting `.ripr/goals/active.toml`.
- Selecting a campaign, issue, branch, writer, or execution wave.
- Reading or copying live GitHub, worktree, claim, or CI state.
- Creating a portable graph that competes with cargo-allow.
- Ranking campaigns or authorizing source, merge, or release work.

## Acceptance Examples

- A newly added reader containing a singleton marker is reported unclassified
  and makes `migration_ready` false.
- A historical closeout reference is classified as
  `historical_campaign_evidence` and does not block migration.
- Two equally specific rules for one path are reported as a contradiction.
- Repeating the audit without input changes preserves the semantic digest.

## Test Mapping

Tests in `xtask/src/active_goal_authority.rs` cover repository completeness,
determinism, hidden singleton readers, legacy-ready non-authorization, and
historical references.

## Implementation Mapping

- `fixtures/active-goal-authority-audit/consumers.toml` — reviewed consumer
  classifications and migration ownership.
- `fixtures/active-goal-authority-audit/issue-contracts.json` — captured issue
  contract inventory.
- `xtask/src/active_goal_authority.rs` — deterministic discovery, validation,
  digest, and JSON/Markdown rendering.
- `plans/codex-orchestration/active-goal-authority-migration.md` — migration
  proof and rollback contract.

## Metrics

The report records consumer, unclassified, unused-inventory, contradiction,
and blocker counts. These are audit denominators, not product telemetry.

## Proof

Focused `xtask` tests plus output-contract, spec-format, traceability,
doc-index, local-context, command-catalog, and generated-clean checks prove the
audit surface without claiming the later authority migration is ready.
