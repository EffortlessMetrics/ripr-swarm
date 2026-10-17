# RIPR-SPEC-0277: Codex adapter delegations bind the selected-work portfolio basis

Status: proposed

Owner: test-infra

Created: 2026-10-17

Linked issues:

- #1706 (parent: bind packets, issue lifecycle and burn-down to
  portfolio-selected work identity; this spec implements the #1637/#1649
  consumer of delivery slice PR B — the Codex adapter/delegation rows)
- #1637 (base project-scoped Codex roles and packet-driven prompts)
- #1649 (issue-lifecycle roles over the issue lifecycle contracts)
- #6864 / RIPR-SPEC-0244 (the shared selected-work identity schemas whose
  captured inputs and law vocabulary this consumer reuses; referenced,
  never duplicated)
- #1704 / #1794 / RIPR-SPEC-0234 (the work-portfolio compiler whose
  captured inputs the check corpus references)
- #1692 (goals-portfolio migration epic)

Support-tier impact:

- None.
- The adapter check is an offline typed validation over committed captured
  bytes. It launches no agent, selects no work, claims nothing, authorizes
  nothing, calls no provider, reads no live GitHub state, and writes only
  `target/ripr/reports/work-adapter-check.{json,md}` plus stdout.

Policy impact:

- None. No new process, network or file-policy surface. The command
  registers in the command mutability catalog as `report_only`; its
  mutation-negative test pins the read-only boundary against the fixture
  tree.

## Problem

#1706 PR A (RIPR-SPEC-0244) defined the shared selected-work identity and
portfolio-basis contract but deliberately migrated no consumer. The
#1637/#1649 Codex adapter still delegated against ad-hoc state: a
subagent could be handed a prompt that re-read the whole portfolio instead
of the selected-work context packet, a builder could be delegated without
a prepared claim or isolated worktree, a subagent could be handed claim
write authority that belongs in the root thread, or a delegation could
bind an issue identity the captured portfolio does not record. This spec
migrates that consumer onto the shared basis: every delegation binds the
repository, the compiled portfolio-snapshot identity, the basis head and
the full selected-work identity it delegates, with one closed project role
and one closed flow stage.

## Behavior

`cargo xtask work adapter check [--corpus <dir>] [--json]` validates
committed `WorkAdapterV1` delegation rows against the same
`PortfolioBasisV1` the RIPR-SPEC-0244 selection check compiles from
immutable captured inputs (default `fixtures/work_adapter_identity`). The
check reuses the shared law engine instead of forking one: each delegation
embeds the `SelectedWorkIdentityV1` it delegates, the selection laws run
on the embedded packet first, and the adapter-level laws then bind the
delegation row. Human Markdown and JSON projections derive from one check
DTO and can never strengthen each other.

### WorkAdapterV1

- `adapter_id`, `repository`, `portfolio_snapshot`, `basis_sha` — the
  bound identities: the delegation must name the basis repository, the
  compiled portfolio-snapshot identity and the captured default-branch
  head.
- `selection` — the full `SelectedWorkIdentityV1` handed to the
  subagent; the RIPR-SPEC-0244 selection laws run on it before any
  adapter-level law, so a delegation cannot smuggle in a selection the
  selection check would reject.
- `role` — one closed project-role wire name: the six base execution
  roles from #1637 (`scout`, `adversary`, `builder`, `verifier`,
  `reviewer`, `cleanup_auditor`) plus the five issue-lifecycle roles from
  #1649 (`issue_triager`, `spec_author`, `spec_adversary`,
  `work_planner`, `burn_down_reviewer`).
- `flow_stage` — one closed stage wire name (`reconstruct`, `select`,
  `compile`, `delegate`, `claim_write`, `verify_review`, `reconcile`).
- `subagent_reads_portfolio` — must stay `false`: a subagent consumes the
  selected-work context packet plus relevant overflow on demand and never
  re-reads the whole portfolio; portfolio compilation stays in the root
  thread.
- `result_schema` — the strict bounded result schema the delegation must
  return (for example `SubtaskResult` or the typed issue/spec/plan/burn-down
  result).
- `derived_from` — retained artifact references the delegation derives
  from; a delegation derived from nothing is a claim, not a delegation.
- `legacy_active_goal_ref` / `legacy_current_work_item_ref` — legacy
  compatibility references resolving read-only to historical campaign
  references; both emit migration posture and grant no write, readiness,
  merge or closeout authority.

The schema has no repository-wide current/default campaign, no mutable
agent assignment, no CI wait state and no progress percentage;
`deny_unknown_fields` rejects additions, and no field on a delegation
grants write, readiness, merge or closeout authority.

### Adapter laws and exact routes

- The embedded selection must satisfy every RIPR-SPEC-0244 law first.
- Repository identity: a delegation for another repository is wrong input
  — `repository_identity` → `recompile_basis`.
- Portfolio-snapshot identity: a delegation binding another snapshot is
  stale — `basis_identity` → `recompile_basis`.
- Basis identity: a recorded basis head that no longer matches the
  captured default-branch head is stale — `basis_identity` →
  `recompile_basis`.
- Subagent portfolio boundary (#1637): `subagent_reads_portfolio: true` —
  `subject_identity` → `reconcile_selection`.
- Write-capable roles (#1637 builder contract): a `builder` delegation
  whose bound selection carries neither a prepared claim nor an isolated
  worktree — `subject_identity` → `reconcile_selection`.
- Claim boundary (#1635): a delegation at the `claim_write` stage —
  `subject_identity` → `reconcile_selection`; subagents never claim, and
  claims stay in the root thread.
- Derived artifacts: an empty `derived_from` — `subject_identity` →
  `reconcile_selection`.
- Legacy compatibility: an unresolvable `legacy_active_goal_ref` or a
  `legacy_current_work_item_ref` that does not match the bound selection's
  issue — `legacy_compatibility` → `reconcile_selection`.

### Captured-input reuse

The committed corpus references the RIPR-SPEC-0244 captured directories by
reference, never duplicated: `fixtures/work_portfolio/corpus` (and the
standalone variant stays available to other corpora). The corpus carries
five scenarios — a read-only scout delegation, a builder delegation with a
prepared claim and isolated worktree, a work-planner compile delegation
over an independent multi-campaign selection, a reviewer delegation
resumed from committed artifacts only, and a six-case wrong-input scenario
— with ten cases total.

### Fixture provenance coverage

`check-fixture-contracts` recomputes the provenance digests fail-closed in
both directions, pins the five required scenario ids, and reruns the check
so every committed case must pass. The corpus must never carry chat or
singleton goal-file authority bytes; resume authority comes from committed
artifacts only.

### Legacy compatibility

Legacy refs resolve read-only and emit migration posture; a legacy
active-goal packet attempting to authorize a delegation fails closed.

## Required Evidence

- The committed corpus in `fixtures/work_adapter_identity` (five
  scenarios, ten cases) with sha256-bound `provenance.json`.
- `cargo test -p xtask --bin xtask work_adapter_identity` green.
- `cargo xtask check-fixture-contracts` green (provenance digests,
  scenario ids, per-case pass).
- The command's mutation-negative test pinning the read-only boundary.

## Inputs

- `fixtures/work_adapter_identity/corpus.json`
- `fixtures/work_adapter_identity/provenance.json`
- The referenced captured directories under `fixtures/work_portfolio` and
  `fixtures/work_selection_identity/captured/` (read-only, by reference).

## Outputs

- `target/ripr/reports/work-adapter-check.json`
- `target/ripr/reports/work-adapter-check.md`
- stdout (Markdown by default, JSON with `--json`).

## Acceptance Examples

- `adapter-02-builder-with-prepared-claim-passes`: a builder delegation
  whose bound selection carries `claim-9102` and `wt-9102` passes.
- `adapter-05-portfolio-reread-and-wrong-inputs-fail` case 0: a delegation
  asserting the subagent re-reads the portfolio fails `subject_identity` →
  `reconcile_selection`.
- `adapter-05` case 1: a builder delegation over a selection with no
  claim or worktree fails `subject_identity` → `reconcile_selection`.
- `adapter-05` case 2: a subagent delegation at the `claim_write` stage
  fails `subject_identity` → `reconcile_selection`.

## Non-Goals

- No delegation spawns a subagent, writes source, merges, closes or grants
  readiness authority.
- No Codex client, model/provider SDK or thread manager integration;
  owned by #1637/#1649 themselves.
- No recursive subagent delegation; direct-child is the supported default.
- No prompt-compiler command; the typed DTO check is this slice.

## Test Mapping

- `work_adapter_identity_committed_corpus_five_scenarios_hold`
- `work_adapter_identity_scout_read_only_delegation_passes`
- `work_adapter_identity_builder_with_prepared_claim_passes`
- `work_adapter_identity_planner_and_resume_review_pass`
- `work_adapter_identity_portfolio_reread_and_wrong_inputs_fail`
- `work_adapter_identity_adapter_schema_has_no_singleton_fields`
- `work_adapter_identity_roles_and_stages_stay_on_the_closed_set`
- `work_adapter_identity_human_and_json_projections_agree`
- `work_adapter_identity_deterministic_output`
- `work_adapter_identity_corpus_loader_fails_closed`
- `work_adapter_identity_committed_provenance_covers_every_corpus_byte`
- `work_adapter_identity_delegations_bind_recorded_issues_and_snapshots`
- `work_adapter_identity_command_mutate_nothing_mutation_negative`

## Implementation Mapping

- `xtask/src/work_adapter_identity.rs` — DTOs, corpus loader, adapter law
  engine, runner, projections, command, fixture validator.
- `xtask/src/work_adapter_identity/tests.rs` — the test mapping above.
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
