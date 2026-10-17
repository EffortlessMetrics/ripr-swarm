# RIPR-SPEC-0276: Plan dispositions bind the selected-work portfolio basis

Status: proposed

Owner: test-infra

Created: 2026-10-17

Linked issues:

- #1706 (parent: bind packets, issue lifecycle and burn-down to
  portfolio-selected work identity; this spec implements the #1646 consumer
  of delivery slice PR B — planning dispositions)
- #1646 (the planning-disposition consumer being migrated here)
- #6864 / RIPR-SPEC-0244 (the shared selected-work identity schemas whose
  captured inputs, law vocabulary and frozen disposition wire names this
  consumer reuses; referenced, never duplicated)
- #1704 / #1794 / RIPR-SPEC-0234 (the work-portfolio compiler whose
  captured inputs the check corpus references)
- #1692 (goals-portfolio migration epic)

Support-tier impact:

- None.
- The plan check is an offline typed validation over committed captured
  bytes. It launches no agent, selects no work, claims nothing, authorizes
  nothing, calls no provider, reads no live GitHub state, and writes only
  `target/ripr/reports/work-plan-check.{json,md}` plus stdout.

Policy impact:

- None. No new process, network or file-policy surface. The command
  registers in the command mutability catalog as `report_only`; its
  mutation-negative test pins the read-only boundary against the fixture
  tree.

## Problem

#1706 PR A (RIPR-SPEC-0244) defined the shared selected-work identity and
portfolio-basis contract but deliberately migrated no consumer. The #1646
planning-disposition consumer still planned against ad-hoc state: a plan
could silently target an unknown campaign, block work without naming the
blocking contract, assert authority to replace or reorder the whole
portfolio, or plan for an issue identity the captured portfolio does not
record. This spec migrates that consumer onto the shared basis: every plan
binds the repository, the compiled portfolio-snapshot identity, the basis
head and the full selected-work identity it plans for, and planning stays
read-only — it can never replace or reorder the portfolio.

## Behavior

`cargo xtask work plan check [--corpus <dir>] [--json]` validates committed
`WorkPlanV1` rows against the same `PortfolioBasisV1` the RIPR-SPEC-0244
selection check compiles from immutable captured inputs (default
`fixtures/work_plan_identity`). The check reuses the shared law engine
instead of forking one: each plan embeds the `SelectedWorkIdentityV1` it
plans for, the selection laws run on the embedded packet first, and the
plan-level laws then bind the plan row. Human Markdown and JSON projections
derive from one check DTO and can never strengthen each other.

### WorkPlanV1

- `plan_id`, `repository`, `portfolio_snapshot`, `basis_sha` — the bound
  identities: the plan must name the basis repository, the compiled
  portfolio-snapshot identity and the captured default-branch head.
- `selection` — the full `SelectedWorkIdentityV1` the plan plans for; the
  RIPR-SPEC-0244 selection laws run on it before any plan-level law, so a
  plan cannot smuggle in a selection the selection check would reject.
- `disposition` — one closed RIPR-SPEC-0244 `WorkPlanDispositionV1` wire
  name (`single_scoped_pr`, `multi_pr_campaign`,
  `append_to_named_campaign`, `standalone_issue_work`,
  `focused_tracker_only`, `already_planned`,
  `blocked_by_contract_or_decision`,
  `root_portfolio_decision_required`).
- `target_campaign` — the campaign targeted by `multi_pr_campaign` /
  `append_to_named_campaign`; it must be a recorded campaign, and no other
  disposition may silently carry one.
- `reorders_portfolio` — read-only planning cannot replace or reorder the
  whole portfolio; a row asserting otherwise fails visibly.
- `blocked_by` — the contract or decision blocking the work, required by
  `blocked_by_contract_or_decision`.
- `derived_from` — retained artifact references the plan derives from; a
  plan derived from nothing is a claim, not a plan.
- `legacy_active_goal_ref` / `legacy_current_work_item_ref` — legacy
  compatibility references resolving read-only to historical campaign
  references; both emit migration posture and grant no write, readiness,
  merge or closeout authority.

The schema has no repository-wide current/default campaign, no mutable
agent assignment, no CI wait state and no progress percentage;
`deny_unknown_fields` rejects additions, and no field on a plan grants
write, readiness, merge or closeout authority.

### Plan laws and exact routes

- The embedded selection must satisfy every RIPR-SPEC-0244 law first.
- Repository identity: a plan for another repository is wrong input —
  `repository_identity` → `recompile_basis`.
- Portfolio-snapshot identity: a plan binding another snapshot is stale —
  `basis_identity` → `recompile_basis`.
- Basis identity: a recorded basis head that no longer matches the captured
  default-branch head is stale — `basis_identity` → `recompile_basis`.
- Read-only boundary: `reorders_portfolio: true` —
  `subject_identity` → `reconcile_selection`.
- Campaign target: a campaign-targeting disposition with no
  `target_campaign`, with an unrecorded target, or a non-targeting
  disposition carrying one — `campaign_ref_identity` →
  `reconcile_selection`.
- Blocked disposition: `blocked_by_contract_or_decision` with an empty
  `blocked_by` — `subject_identity` → `reconcile_selection`.
- Derived artifacts: an empty `derived_from` — `subject_identity` →
  `reconcile_selection`.
- Legacy compatibility: an unresolvable `legacy_active_goal_ref` or a
  `legacy_current_work_item_ref` that does not match the bound selection's
  issue — `legacy_compatibility` → `reconcile_selection`.

### Captured-input reuse

The committed corpus references the RIPR-SPEC-0244 captured directories by
reference, never duplicated: `fixtures/work_portfolio/corpus` and
`fixtures/work_selection_identity/captured/standalone`. The corpus carries
five scenarios (single scoped PR, standalone issue, append to a named
recorded campaign, blocked by a named contract, and a six-case wrong-input
scenario) with ten cases total.

### Fixture provenance coverage

`check-fixture-contracts` recomputes the provenance digests fail-closed in
both directions, pins the five required scenario ids, and reruns the check
so every committed case must pass. The corpus must never carry chat or
singleton goal-file authority bytes; resume authority comes from committed
artifacts only.

### Legacy compatibility

Legacy refs resolve read-only and emit migration posture; a legacy
active-goal packet attempting to authorize planning fails closed.

## Required Evidence

- The committed corpus in `fixtures/work_plan_identity` (five scenarios,
  ten cases) with sha256-bound `provenance.json`.
- `cargo test -p xtask --bin xtask work_plan_identity` green.
- `cargo xtask check-fixture-contracts` green (provenance digests,
  scenario ids, per-case pass).
- The command's mutation-negative test pinning the read-only boundary.

## Inputs

- `fixtures/work_plan_identity/corpus.json`
- `fixtures/work_plan_identity/provenance.json`
- The referenced captured directories under `fixtures/work_portfolio` and
  `fixtures/work_selection_identity/captured/` (read-only, by reference).

## Outputs

- `target/ripr/reports/work-plan-check.json`
- `target/ripr/reports/work-plan-check.md`
- stdout (Markdown by default, JSON with `--json`).

## Acceptance Examples

- `plan-03-append-to-named-campaign-disposition-passes`: an append
  disposition naming the recorded `campaign-rust-repair` passes.
- `plan-05-reorder-and-wrong-inputs-fail` case 0: a plan asserting it
  reorders the portfolio fails `subject_identity` → `reconcile_selection`.
- `plan-05` case 2: an append disposition naming `campaign-unknown` fails
  `campaign_ref_identity` → `reconcile_selection`.
- `plan-05` case 5: a legacy goal ref that resolves to no recorded
  historical campaign fails `legacy_compatibility` → `reconcile_selection`.

## Non-Goals

- No plan selects work, claims a writer or grants
  readiness/merge/closeout authority.
- No campaign ranking or portfolio rebalancing; owned by #1704.
- No autonomous plan execution, agent spawning, source edit, merge or
  closeout.
- No adapter consumer migrates in this slice.

## Test Mapping

- `work_plan_identity_committed_corpus_five_scenarios_hold`
- `work_plan_identity_single_scoped_pr_disposition_passes`
- `work_plan_identity_standalone_and_append_dispositions_pass`
- `work_plan_identity_blocked_disposition_passes`
- `work_plan_identity_reorder_and_wrong_inputs_fail`
- `work_plan_identity_plan_schema_has_no_singleton_fields`
- `work_plan_identity_dispositions_stay_on_the_closed_set`
- `work_plan_identity_human_and_json_projections_agree`
- `work_plan_identity_deterministic_output`
- `work_plan_identity_corpus_loader_fails_closed`
- `work_plan_identity_committed_provenance_covers_every_corpus_byte`
- `work_plan_identity_plans_bind_recorded_issues_and_snapshots`
- `work_plan_identity_command_mutate_nothing_mutation_negative`

## Implementation Mapping

- `xtask/src/work_plan_identity.rs` — DTOs, corpus loader, plan law engine,
  runner, projections, command, fixture validator.
- `xtask/src/work_plan_identity/tests.rs` — the test mapping above.
- `xtask/src/work_selection_identity.rs` — the reused RIPR-SPEC-0244 law
  vocabulary, selection law engine and provenance DTO (referenced, never
  forked).
- `xtask/src/work_portfolio.rs` — the reused RIPR-SPEC-0234 compiler and
  captured-input loaders.
- `xtask/src/command.rs`, `xtask/src/dispatch.rs`, `xtask/src/main.rs`,
  `xtask/src/fixture_contracts/mod.rs`, `xtask/src/reports/fixtures.rs` —
  wiring and registrations.

## Metrics

- unit_test_pass_rate
- fixture corpus pass rate (pinned at 100% by `check-fixture-contracts`)
