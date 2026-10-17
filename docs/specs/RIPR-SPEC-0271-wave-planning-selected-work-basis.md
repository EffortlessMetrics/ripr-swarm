# RIPR-SPEC-0271: Execution-wave planning partitions the selected candidate set over explicit resources

Status: proposed

Owner: test-infra

Created: 2026-10-09

Linked issues:

- #1706 (parent: bind packets, issue lifecycle and burn-down to
  portfolio-selected work identity; this spec implements the #1634 consumer
  of delivery slice PR B — wave planning)
- #1634 (the wave-planning consumer migrated here)
- #6864 / RIPR-SPEC-0244 (the shared selected-work identity schemas this
  consumer reuses: `PortfolioBasisV1`, `SelectedWorkIdentityV1`, the sealed
  law/route vocabulary and the identity law engine; referenced, never
  duplicated)
- #1704 / #1794 / RIPR-SPEC-0234 (the work-portfolio compiler whose
  captured inputs the check corpus references)
- #1692 (goals-portfolio migration epic)

Support-tier impact:

- None.
- The wave check is an offline typed validation over committed captured
  bytes. It launches no agent, selects no work, claims nothing, admits no
  wave, calls no provider, reads no live GitHub state, and writes only
  `target/ripr/reports/work-wave-check.{json,md}` plus stdout.

Policy impact:

- None. No new process, network or file-policy surface. The command
  registers in the command mutability catalog as `report_only`; its
  mutation-negative test pins the read-only boundary against the fixture
  tree.

## Problem

#1706 PR A (RIPR-SPEC-0244) defined the shared selected-work identity and
portfolio-basis contract but deliberately migrated no consumer. The #1634
wave-planning train must move off the singleton: without a migrated plan
schema, a generic global `active-goal` resource could keep serializing
unrelated work, one campaign pointer could keep deciding which waves may
run, and incomplete plans could silently drop selected candidates or
schedule the same exclusive resource in two waves at once. PR B must make
every plan consume the complete selected candidate set and
capacity/conflict facts, must remove the generic `active-goal` resource in
favor of explicit resources, and must admit independent waves across
campaigns when semantic/physical resources and capacity are disjoint.

## Behavior

`cargo xtask work wave check [--corpus <dir>] [--json]` loads the
committed wave-planning corpus (default `fixtures/work_wave_identity`) and,
for each scenario, compiles the referenced captured portfolio directory
with the unchanged RIPR-SPEC-0234 compiler into a `PortfolioBasisV1`, then
runs the wave law engine over each committed `WorkWavePlanV1` row. Human
Markdown and JSON projections derive from one check DTO and agree on
scenario, case and pass/fail counts; neither can strengthen the other.

### WorkWavePlanV1

Every plan binds, reusing the RIPR-SPEC-0244 DTOs rather than forking
them: the plan id, the repository, the compiled portfolio-snapshot
identity, the basis SHA, the complete selected candidate set (full
`SelectedWorkIdentityV1` each), the declared writer capacity, the waves,
and the legacy compatibility reference. The schema has no repository-wide
current/default campaign and no generic goal resource;
`deny_unknown_fields` rejects any such addition.

### Explicit resource vocabulary

A wave names only explicit resources: `campaign-record:<id>` (the id must
be a recorded campaign), `portfolio-schema`, `spec-ledger`,
`output-contract:<id>`, `issue:<n>`, `pr:<n>` and `worktree:<path>` (the
path must be a captured local worktree). `portfolio-schema` and
`spec-ledger` are read-only shared references: sharing them never
serializes unrelated work. Every other resource is exclusive. The retired
generic `active-goal` spelling — and every non-explicit shape — fails
closed with `campaign_ref_identity`/`reconcile_resources` and names the
explicit vocabulary.

### Wave laws and exact routes

Every embedded selection must satisfy every RIPR-SPEC-0244 identity law
first — the shared engine runs inside the wave check, so a plan cannot
smuggle in a packet the selection check would reject. The plan-level laws
bind the repository, portfolio-snapshot identity and basis head (each
`recompile_basis`). The wave-level laws then bind: wave membership (a wave
names only selected candidates, `subject_identity`/`reconcile_selection`),
coverage (every selected candidate is assigned to exactly one wave,
`subject_identity`/`reconcile_selection`), per-wave capacity (a wave
cannot exceed the declared writer capacity,
`worktree_identity`/`reconcile_resources`), the resource vocabulary, and
disjointness of exclusive resources across concurrent waves
(`overlap_visibility`/`reconcile_resources`, naming both waves and the
shared resource). Disjoint cross-campaign waves — disjoint exclusive
resources, shareable read-only schema/ledger references, capacity within
bounds — are admitted. Legacy `legacy_active_goal_ref` on a plan resolves
read-only to a historical campaign reference, emits migration posture, and
grants no write, readiness, merge or closeout authority.

### Captured-input reuse

The corpus references the committed RIPR-SPEC-0244 captured directories
(`fixtures/work_portfolio/corpus`) by relative path; captured bytes are
never duplicated, and scenario captured paths keep the same fail-closed
resolution rules as the selection check.

### Fixture provenance coverage

`fixtures/work_wave_identity/provenance.json` binds every corpus JSON
byte with SHA-256 in the RIPR-SPEC-0223 convention under the
`work_wave_identity_provenance.v1` schema version; unlisted fixture files
fail closed, and provenance paths reject absolute, parent-relative and
backslash spellings on every platform.

## Required Evidence

- The committed corpus `fixtures/work_wave_identity/corpus.json` with all
  six required scenarios and per-case pinned expectations.
- `fixtures/work_wave_identity/provenance.json` SHA-256 bindings,
  recomputed fail-closed by `cargo xtask check-fixture-contracts`.
- `cargo xtask work wave check` passing on the committed corpus, with
  byte-stable JSON output and a stable portable identity across runs.
- `cargo test -p xtask work_wave_identity` pinning the acceptance items
  this slice can honestly pin.

## Inputs

- The committed wave-planning corpus (`corpus.json`) and the captured
  portfolio directories it references under `fixtures/`.

## Outputs

- `work_wave_check_view.v1` JSON and the derived Markdown on stdout and
  under `target/ripr/reports/work-wave-check.{json,md}`.

## Acceptance Examples

- A plan partitioning the editor-ux and rust-repair selections into two
  waves over disjoint exclusive resources is admitted, although both waves
  share the read-only `portfolio-schema` and `spec-ledger` references.
- The narrow single-agent issue `9202` gets its own single-selection wave
  and is admitted while another campaign's wave stays active.
- A plan whose waves share the exclusive `output-contract:preview-panel`
  fails `overlap_visibility`/`reconcile_resources` naming both waves.
- A wave naming the retired `active-goal` resource fails
  `campaign_ref_identity`/`reconcile_resources` and names the explicit
  vocabulary; a non-explicit shape fails the same way.
- A plan with a stale basis fails `basis_identity`/`recompile_basis`; a
  plan that leaves a selected candidate unassigned fails
  `subject_identity`/`reconcile_selection` naming the candidate.
- A wave scheduling more selections than the declared writer capacity
  fails `worktree_identity`/`reconcile_resources`.
- The human Markdown projection renders the same snake_case wire names as
  the JSON projection; `Debug` spellings never appear.

## Non-Goals

- No claim, synthesis, issue-intake or burn-down migration: #1633 lands in
  RIPR-SPEC-0269, #1635/#1647 in RIPR-SPEC-0270, and #1636/#1644/#1648
  and the #1637/#1649 adapters keep their current behavior and land in
  later slices.
- No wave execution, scheduling or agent spawning; the check validates
  committed plan rows only and admits no wave into any runtime.
- No autonomous selection or campaign placement; plans consume the
  candidate set the root selected.
- No live GitHub, label, issue-age or title-keyword input; committed
  captured bytes are the only authority.

Requirement-level v2 blocks and PR-local implementation slices belong in
their respective authorities; do not duplicate their normative prose here.
Acceptance of this document does not imply implementation, evidence, or
support.

## Test Mapping

- `xtask/src/work_wave_identity.rs::tests::work_wave_identity_committed_corpus_six_scenarios_hold`
- `xtask/src/work_wave_identity.rs::tests::work_wave_identity_disjoint_cross_campaign_waves_pass`
- `xtask/src/work_wave_identity.rs::tests::work_wave_identity_retired_active_goal_resource_fails_closed`
- `xtask/src/work_wave_identity.rs::tests::work_wave_identity_stale_basis_and_unassigned_candidate_fail`
- `xtask/src/work_wave_identity.rs::tests::work_wave_identity_resource_conflicts_fail_with_exact_routes`
- `xtask/src/work_wave_identity.rs::tests::work_wave_identity_plan_schema_has_no_singleton_fields`
- `xtask/src/work_wave_identity.rs::tests::work_wave_identity_embedded_selection_reuses_shared_laws`
- `xtask/src/work_wave_identity.rs::tests::work_wave_identity_human_and_json_projections_agree`
- `xtask/src/work_wave_identity.rs::tests::work_wave_identity_deterministic_output`
- `xtask/src/work_wave_identity.rs::tests::work_wave_identity_corpus_loader_fails_closed`
- `xtask/src/work_wave_identity.rs::tests::work_wave_identity_provenance_fails_closed`
- `xtask/src/work_wave_identity.rs::tests::work_wave_identity_committed_provenance_covers_every_corpus_byte`
- `xtask/src/work_wave_identity.rs::tests::work_wave_identity_command_mutate_nothing_mutation_negative`

## Implementation Mapping

- `xtask/src/work_wave_identity.rs` — plan DTOs, the resource vocabulary
  classifier, the wave law engine (reusing the RIPR-SPEC-0244 engine for
  the embedded selections), corpus runner, projections, command, provenance
  loader and fixture validator.
- `xtask/src/command.rs`, `xtask/src/dispatch.rs` — `work wave check`
  registration in the parser, dispatcher and the command catalog.
- `xtask/src/fixture_contracts/mod.rs`, `xtask/src/reports/fixtures.rs` —
  corpus validator registration and the manifest-only fixture exemption.

## Metrics

- `unit_test_pass_rate` over the `work_wave_identity` test prefix.
- Corpus expectation coverage: 6/6 scenarios, 7/7 pinned cases passing.
