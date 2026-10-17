# RIPR-SPEC-0270: Durable claims bind one selected item and the exact portfolio basis

Status: proposed

Owner: test-infra

Created: 2026-10-09

Linked issues:

- #1706 (parent: bind packets, issue lifecycle and burn-down to
  portfolio-selected work identity; this spec implements the #1635/#1647
  consumer of delivery slice PR B — local and durable claims)
- #1635 / #1647 (the claim-binding consumers migrated here)
- #6864 / RIPR-SPEC-0244 (the shared selected-work identity schemas this
  consumer reuses: `PortfolioBasisV1`, `SelectedWorkIdentityV1`, the sealed
  law/route vocabulary and the identity law engine; referenced, never
  duplicated)
- #1704 / #1794 / RIPR-SPEC-0234 (the work-portfolio compiler whose
  captured inputs the check corpus references)
- #1692 (goals-portfolio migration epic)

Support-tier impact:

- None.
- The claim check is an offline typed validation over committed captured
  bytes. It launches no agent, selects no work, claims nothing, authorizes
  no writer, calls no provider, reads no live GitHub state, and writes only
  `target/ripr/reports/work-claim-check.{json,md}` plus stdout.

Policy impact:

- None. No new process, network or file-policy surface. The command
  registers in the command mutability catalog as `report_only`; its
  mutation-negative test pins the read-only boundary against the fixture
  tree.

## Problem

#1706 PR A (RIPR-SPEC-0244) defined the shared selected-work identity and
portfolio-basis contract but deliberately migrated no consumer. The #1635
local-claims and #1647 durable-claims trains must bind one selected
issue/work item/action and the exact portfolio basis. Without a migrated
schema, claims could keep asserting repository-wide priority from one
campaign pointer, a stale basis could silently release or hand over a
writer, and one generic conflict resource could serialize unrelated work.
PR B must make every claim bind exactly one selection and basis, must keep
claiming one item from asserting repository-wide priority, and must route
every stale basis and every resource conflict to reconciliation — never
automatic release or takeover.

## Behavior

`cargo xtask work claim check [--corpus <dir>] [--json]` loads the
committed durable-claim corpus (default `fixtures/work_claim_identity`) and,
for each scenario, compiles the referenced captured portfolio directory
with the unchanged RIPR-SPEC-0234 compiler into a `PortfolioBasisV1`, then
runs the claim law engine over each committed `WorkClaimV1` row in order.
Claims that pass join the scenario's committed claim set and constrain
later claims. Human Markdown and JSON projections derive from one check DTO
and agree on scenario, case and pass/fail counts; neither can strengthen
the other.

### WorkClaimV1

Every claim row binds, reusing the RIPR-SPEC-0244 DTOs rather than forking
them: the claim id (a durable identity, never a reusable label), the
repository, the compiled portfolio-snapshot identity, the basis SHA, the
full `SelectedWorkIdentityV1` the claim binds, the writer worktree the
claim occupies, the closed claim-kind vocabulary (`writer`), and the legacy
compatibility references. The schema has no repository-wide
current/default campaign, no priority field, no mutable agent assignment,
no CI wait state and no progress percentage; `deny_unknown_fields`
rejects any such addition, so claiming one item can never assert
repository-wide priority.

### Claim laws, interactions and exact routes

The embedded selection must satisfy every RIPR-SPEC-0244 identity law
first — the shared engine runs inside the claim check. The claim-level
laws then bind the repository (`recompile_basis`), the portfolio-snapshot
identity (`recompile_basis`) and the basis head (`recompile_basis`). A
stale basis keeps the claim recorded with state
`stale_requires_reconcile`: it is not released and no other claim takes
over its worktree. The occupied worktree must be recorded in the selection
overlaps (`worktree_identity`/`reconcile_resources`). The interaction laws
bind the claim set: a duplicate claim id fails
(`subject_identity`/`reconcile_selection`); one worktree cannot serve two
selections at once (`worktree_identity`/`reconcile_resources`); one
selection cannot be claimed by two worktrees
(`subject_identity`/`reconcile_selection`). Every conflict names the
occupying claim and leaves it active — reconciliation is the only route,
never release or takeover. Legacy `legacy_active_goal_ref` /
`legacy_current_work_item_ref` fields on a claim resolve read-only to
historical campaign references, emit migration posture, and grant no
write, readiness, merge or closeout authority; an unresolvable legacy
reference routes `reconcile_selection`.

### Captured-input reuse

The corpus references the committed RIPR-SPEC-0244 captured directories
(`fixtures/work_portfolio/corpus` and
`fixtures/work_selection_identity/captured/*`) by relative path; captured
bytes are never duplicated, and scenario captured paths keep the same
fail-closed resolution rules as the selection check.

### Fixture provenance coverage

`fixtures/work_claim_identity/provenance.json` binds every corpus JSON
byte with SHA-256 in the RIPR-SPEC-0223 convention under the
`work_claim_identity_provenance.v1` schema version; unlisted fixture files
fail closed, and provenance paths reject absolute, parent-relative and
backslash spellings on every platform.

## Required Evidence

- The committed corpus `fixtures/work_claim_identity/corpus.json` with all
  five required scenarios and per-case pinned expectations.
- `fixtures/work_claim_identity/provenance.json` SHA-256 bindings,
  recomputed fail-closed by `cargo xtask check-fixture-contracts`.
- `cargo xtask work claim check` passing on the committed corpus, with
  byte-stable JSON output and a stable portable identity across runs.
- `cargo test -p xtask work_claim_identity` pinning the acceptance items
  this slice can honestly pin.

## Inputs

- The committed durable-claim corpus (`corpus.json`) and the captured
  portfolio directories it references under `fixtures/`.

## Outputs

- `work_claim_check_view.v1` JSON and the derived Markdown on stdout and
  under `target/ripr/reports/work-claim-check.{json,md}`.

## Acceptance Examples

- Writer claims for three distinct selections on three distinct worktrees
  all pass and join the committed claim set.
- A standalone selection with zero campaign references is claimable.
- A claim occupying a worktree already claimed for another selection
  fails `worktree_identity`/`reconcile_resources`, names the occupying
  claim, and leaves it active.
- A second claim on an already-claimed selection fails
  `subject_identity`/`reconcile_selection` and leaves the first claim
  active.
- A claim recorded against a stale basis fails
  `basis_identity`/`recompile_basis` and stays recorded as
  `stale_requires_reconcile`; it is not released and its worktree is not
  taken over.
- A claim carrying `legacy_active_goal_ref: "campaign-rust-repair"`
  passes read-only, emits migration posture, and grants no authority; a
  mismatched legacy work-item ref fails
  `legacy_compatibility`/`reconcile_selection`.
- The human Markdown projection renders the same snake_case wire names as
  the JSON projection; `Debug` spellings never appear.

## Non-Goals

- No task/result, wave-planning, synthesis, issue-intake or burn-down
  migration: #1633 lands in RIPR-SPEC-0269, #1634/#1636/#1644/#1648 and
  the #1637/#1649 adapters keep their current behavior and land in later
  slices.
- No claim enforcement against live worktrees: the check validates
  committed corpus rows only and mutates no claim store.
- No autonomous claim creation, release, takeover or agent assignment.
- No live GitHub, label, issue-age or title-keyword input; committed
  captured bytes are the only authority.

Requirement-level v2 blocks and PR-local implementation slices belong in
their respective authorities; do not duplicate their normative prose here.
Acceptance of this document does not imply implementation, evidence, or
support.

## Test Mapping

- `xtask/src/work_claim_identity.rs::tests::work_claim_identity_committed_corpus_five_scenarios_hold`
- `xtask/src/work_claim_identity.rs::tests::work_claim_identity_distinct_and_standalone_claims_pass`
- `xtask/src/work_claim_identity.rs::tests::work_claim_identity_stale_basis_reconciles_without_takeover`
- `xtask/src/work_claim_identity.rs::tests::work_claim_identity_conflicts_fail_with_exact_routes`
- `xtask/src/work_claim_identity.rs::tests::work_claim_identity_legacy_claims_read_only_no_authority`
- `xtask/src/work_claim_identity.rs::tests::work_claim_identity_claim_schema_has_no_singleton_fields`
- `xtask/src/work_claim_identity.rs::tests::work_claim_identity_duplicate_claim_id_fails_closed`
- `xtask/src/work_claim_identity.rs::tests::work_claim_identity_human_and_json_projections_agree`
- `xtask/src/work_claim_identity.rs::tests::work_claim_identity_deterministic_output`
- `xtask/src/work_claim_identity.rs::tests::work_claim_identity_corpus_loader_fails_closed`
- `xtask/src/work_claim_identity.rs::tests::work_claim_identity_provenance_fails_closed`
- `xtask/src/work_claim_identity.rs::tests::work_claim_identity_committed_provenance_covers_every_corpus_byte`
- `xtask/src/work_claim_identity.rs::tests::work_claim_identity_command_mutate_nothing_mutation_negative`

## Implementation Mapping

- `xtask/src/work_claim_identity.rs` — claim DTOs, the claim law engine
  with claim-set interactions (reusing the RIPR-SPEC-0244 engine for the
  embedded selection), corpus runner, projections, command, provenance
  loader and fixture validator.
- `xtask/src/command.rs`, `xtask/src/dispatch.rs` — `work claim check`
  registration in the parser, dispatcher and the command catalog.
- `xtask/src/fixture_contracts/mod.rs`, `xtask/src/reports/fixtures.rs` —
  corpus validator registration and the manifest-only fixture exemption.

## Metrics

- `unit_test_pass_rate` over the `work_claim_identity` test prefix.
- Corpus expectation coverage: 5/5 scenarios, 9/9 pinned cases passing.
