# RIPR-SPEC-0269: Task/result/receipt schemas bind the selected-work portfolio basis

Status: proposed

Owner: test-infra

Created: 2026-10-09

Linked issues:

- #1706 (parent: bind packets, issue lifecycle and burn-down to
  portfolio-selected work identity; this spec implements the #1633 consumer
  of delivery slice PR B — task/result/receipt schemas)
- #1633 (the task/result/receipt consumer being migrated here)
- #6864 / RIPR-SPEC-0244 (the shared selected-work identity schemas this
  consumer reuses: `PortfolioBasisV1`, `SelectedWorkIdentityV1`, the sealed
  law/route vocabulary and the identity law engine; referenced, never
  duplicated)
- #1704 / #1794 / RIPR-SPEC-0234 (the work-portfolio compiler whose
  captured inputs the check corpus references)
- #1692 (goals-portfolio migration epic)

Support-tier impact:

- None.
- The result check is an offline typed validation over committed captured
  bytes. It launches no agent, selects no work, claims nothing, authorizes
  nothing, calls no provider, reads no live GitHub state, and writes only
  `target/ripr/reports/work-result-check.{json,md}` plus stdout.

Policy impact:

- None. No new process, network or file-policy surface. The command
  registers in the command mutability catalog as `report_only`; its
  mutation-negative test pins the read-only boundary against the fixture
  tree.

## Problem

#1706 PR A (RIPR-SPEC-0244) defined the shared selected-work identity and
portfolio-basis contract but deliberately migrated no consumer. The #1633
task/result/receipt train is the first consumer that must move: without a
migrated schema, a result could still be accepted for another repository,
an older portfolio snapshot, a different issue or action, a stale basis, an
unrecorded worktree or a wrong exact head, recreating the singleton
indirectly — one campaign pointer or one active-goal digest could keep
deciding whose results count. PR B must make every packet/result bind the
repository, the portfolio-snapshot identity, the basis head and the full
selected-work identity it extends, and must fail stale/wrong input visibly
with the exact recompile/reconcile route.

## Behavior

`cargo xtask work result check [--corpus <dir>] [--json]` loads the
committed task-result corpus (default `fixtures/work_result_identity`) and,
for each scenario, compiles the referenced captured portfolio directory
with the unchanged RIPR-SPEC-0234 compiler into a `PortfolioBasisV1`, then
runs the result law engine over each committed `WorkTaskResultV1` row.
Human Markdown and JSON projections derive from one check DTO and agree on
scenario, case and pass/fail counts; neither can strengthen the other.

### WorkTaskResultV1

Every result row binds, reusing the RIPR-SPEC-0244 DTOs rather than forking
them: the result id, the repository, the compiled portfolio-snapshot
identity, the basis SHA, the full `SelectedWorkIdentityV1` the result
extends, the closed result-kind vocabulary (`context_compiled` |
`task_dispatched` | `build_completed` | `verification_recorded` |
`review_recorded`), the writer worktree when any, the exact reported head,
the retained evidence references, and the legacy compatibility references.
The schema has no repository-wide current/default campaign, no mutable
agent assignment, no CI wait state and no progress percentage;
`deny_unknown_fields` rejects any such addition.

### Result laws and exact routes

The embedded selection must satisfy every RIPR-SPEC-0244 identity law
first — the shared engine runs inside the result check, so a result cannot
smuggle in a packet the selection check would reject. The result-level
laws then bind: a result naming another repository routes
`recompile_basis`; a result binding another portfolio-snapshot identity is
stale and routes `recompile_basis`; a recorded basis SHA that no longer
matches the captured default-branch head routes `recompile_basis`; a
reported head that does not match the selection's expected head routes
`reconcile_head`; a claimed worktree the selection does not record in its
overlaps routes `reconcile_resources`; an empty evidence list routes
`reconcile_selection` (a result with no evidence reference is a claim, not
a result). Legacy `legacy_active_goal_ref` /
`legacy_current_work_item_ref` fields on a result resolve read-only to
historical campaign references, emit migration posture, and grant no
write, readiness, merge or closeout authority; an unresolvable legacy
reference routes `reconcile_selection`.

### Captured-input reuse

The corpus references the committed RIPR-SPEC-0244 captured directories
(`fixtures/work_portfolio/corpus` and
`fixtures/work_selection_identity/captured/*`) by relative path; captured
bytes are never duplicated, and scenario captured paths keep the same
fail-closed resolution rules as the selection check (corpus-local first,
then the repository `fixtures/` root; absolute, parent-relative and
backslash spellings fail closed on every platform).

### Fixture provenance coverage

`fixtures/work_result_identity/provenance.json` binds every corpus JSON
byte with SHA-256 in the RIPR-SPEC-0223 convention; unlisted fixture files
fail closed, and provenance paths reject absolute, parent-relative and
backslash spellings on every platform.

### Legacy compatibility

Legacy references are read-only compatibility state. They never promote
readiness, proof, support or completion, and a result row that would
authorize a writer from a legacy reference has no field to express it — the
closed vocabulary and `deny_unknown_fields` keep it unrepresentable.

## Required Evidence

- The committed corpus `fixtures/work_result_identity/corpus.json` with all
  five required scenarios and per-case pinned expectations.
- `fixtures/work_result_identity/provenance.json` SHA-256 bindings,
  recomputed fail-closed by `cargo xtask check-fixture-contracts`.
- `cargo xtask work result check` passing on the committed corpus, with
  byte-stable JSON output and a stable portable identity across runs.
- `cargo test -p xtask work_result_identity` pinning the acceptance items
  this slice can honestly pin.

## Inputs

- The committed task-result corpus (`corpus.json`) and the captured
  portfolio directories it references under `fixtures/`.

## Outputs

- `work_result_check_view.v1` JSON and the derived Markdown on stdout and
  under `target/ripr/reports/work-result-check.{json,md}`.

## Acceptance Examples

- A build result binding the issue `9105` `start_build` selection against
  the canonical corpus passes with one member campaign reference, the
  matching snapshot identity, the matching basis head and evidence refs.
- The same result shape against the standalone captured variant passes for
  issue `9106` with zero campaign references.
- A repair result for issue `9101` binds the worktree `<root>/wt-9101` the
  selection recorded and passes.
- A result carrying `legacy_active_goal_ref: "campaign-rust-repair"`
  passes read-only, emits migration posture, and grants no authority.
- A result naming another repository, another portfolio snapshot, a stale
  basis, a wrong reported head, an unrecorded worktree, an embedded action
  that mismatches the portfolio candidate, no evidence reference, or an
  unresolvable legacy ref each fails with the exact pinned law/route pair.
- The human Markdown projection renders the same snake_case wire names as
  the JSON projection; `Debug` spellings never appear.

## Non-Goals

- No claim, wave-planning, synthesis, issue-intake or burn-down migration:
  #1635/#1647, #1634, #1636, #1644 and #1648 keep their current behavior
  and land in later slices.
- No portfolio candidate ranking (owned by #1704), no autonomous selection,
  no agent spawning, no source edit, merge or closeout authority.
- No live GitHub, label, issue-age or title-keyword input; committed
  captured bytes are the only authority.

Requirement-level v2 blocks and PR-local implementation slices belong in
their respective authorities; do not duplicate their normative prose here.
Acceptance of this document does not imply implementation, evidence, or
support.

## Test Mapping

- `xtask/src/work_result_identity.rs::tests::work_result_identity_committed_corpus_five_scenarios_hold`
- `xtask/src/work_result_identity.rs::tests::work_result_identity_wrong_inputs_fail_stale_or_wrong_input`
- `xtask/src/work_result_identity.rs::tests::work_result_identity_embedded_selection_reuses_shared_laws`
- `xtask/src/work_result_identity.rs::tests::work_result_identity_result_schema_has_no_singleton_fields`
- `xtask/src/work_result_identity.rs::tests::work_result_identity_valid_results_bind_their_selections`
- `xtask/src/work_result_identity.rs::tests::work_result_identity_legacy_refs_read_only_no_authority`
- `xtask/src/work_result_identity.rs::tests::work_result_identity_human_and_json_projections_agree`
- `xtask/src/work_result_identity.rs::tests::work_result_identity_deterministic_output`
- `xtask/src/work_result_identity.rs::tests::work_result_identity_corpus_loader_fails_closed`
- `xtask/src/work_result_identity.rs::tests::work_result_identity_committed_provenance_covers_every_corpus_byte`
- `xtask/src/work_result_identity.rs::tests::work_result_identity_command_mutate_nothing_mutation_negative`
- `xtask/src/work_result_identity.rs::tests::work_result_identity_basis_facts_for_corpus_authoring`

## Implementation Mapping

- `xtask/src/work_result_identity.rs` — result DTOs, the result law engine
  (reusing the RIPR-SPEC-0244 engine for the embedded selection), corpus
  runner, projections, command, provenance loader and fixture validator.
- `xtask/src/work_selection_identity.rs` — `build_portfolio_basis`,
  `check_packet`, `CaseEvaluation`, `corpus_root`,
  `resolve_scenario_captured` and `selection_wire_name` widened to
  `pub(crate)`; the selection law engine itself is unchanged.
- `xtask/src/command.rs`, `xtask/src/dispatch.rs` — `work result check`
  registration in the parser, dispatcher and the command catalog.
- `xtask/src/fixture_contracts/mod.rs`, `xtask/src/reports/fixtures.rs` —
  corpus validator registration and the manifest-only fixture exemption.

## Metrics

- `unit_test_pass_rate` over the `work_result_identity` test prefix.
- Corpus expectation coverage: 5/5 scenarios, 14/14 pinned cases passing.
