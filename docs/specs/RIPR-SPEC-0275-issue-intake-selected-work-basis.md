# RIPR-SPEC-0275: Issue intake binds the selected-work portfolio basis

Status: proposed

Owner: test-infra

Created: 2026-10-17

Linked issues:

- #1706 (parent: bind packets, issue lifecycle and burn-down to
  portfolio-selected work identity; this spec implements the #1644 consumer
  of delivery slice PR B — issue intake)
- #1644 (the issue intake consumer being migrated here)
- #6864 / RIPR-SPEC-0244 (the shared selected-work identity schemas whose
  captured inputs and law vocabulary this consumer reuses; referenced,
  never duplicated)
- #1704 / #1794 / RIPR-SPEC-0234 (the work-portfolio compiler whose
  captured inputs the check corpus references)
- #1692 (goals-portfolio migration epic)

Support-tier impact:

- None.
- The intake check is an offline typed validation over committed captured
  bytes. It launches no agent, selects no work, claims nothing, authorizes
  nothing, calls no provider, reads no live GitHub state, and writes only
  `target/ripr/reports/work-intake-check.{json,md}` plus stdout.

Policy impact:

- None. No new process, network or file-policy surface. The command
  registers in the command mutability catalog as `report_only`; its
  mutation-negative test pins the read-only boundary against the fixture
  tree.

## Problem

#1706 PR A (RIPR-SPEC-0244) defined the shared selected-work identity and
portfolio-basis contract but deliberately migrated no consumer. The #1644
issue intake train still consumed one active-goal input: without a
migrated schema, triage could prefer or block work based on one campaign
pointer, and an issue outside every campaign could stay unrepresentable.
PR B must make intake consume a portfolio snapshot, relevant
campaigns/plans, open overlapping PRs and claims, the current main and
accepted contracts; an out-of-campaign issue must still qualify, with
placement remaining a later root/planning decision.

## Behavior

`cargo xtask work intake check [--corpus <dir>] [--json]` loads the
committed issue-intake corpus (default `fixtures/work_intake_identity`)
and, for each scenario, compiles the referenced captured portfolio
directory with the unchanged RIPR-SPEC-0234 compiler into a
`PortfolioBasisV1`, then runs the intake law engine over each committed
`WorkIntakeV1` row. Human Markdown and JSON projections derive from one
check DTO and agree on scenario, case and pass/fail counts; neither can
strengthen the other.

### WorkIntakeV1

Every intake row binds:

- `intake_id` — a stable intake identity string.
- `repository` — must equal the basis repository.
- `portfolio_snapshot` — the compiled snapshot identity; another snapshot
  is stale.
- `basis_sha` — the captured default-branch head; another head is stale.
- `current_main_sha` — the current main the intake derives against; it
  must match the captured default-branch head.
- `issue_number` — the issue being qualified; it must be recorded in the
  captured issue source.
- `qualification` — a closed vocabulary: `qualified` or `needs_triage`.
- `campaign_placement` — the campaign placement under consideration, when
  any; it must be a recorded campaign. `None` means standalone: valid
  triage input, never a blocker.
- `relevant_campaigns` — relevant campaign/plan references; each must be a
  recorded campaign.
- `open_overlaps` — open overlapping PRs and claims that must stay visible
  before any mutation.
- `accepted_contracts` — accepted contracts the intake was qualified
  against; an empty list qualifies nothing.
- `single_agent` — whether the captured issue prefers a single agent.
- `derived_from` — retained artifact references the intake derives from;
  an empty list is a claim, not an intake.
- `legacy_active_goal_ref` / `legacy_current_work_item_ref` — read-only
  compatibility references (see Legacy compatibility).

### Intake laws and exact routes

The intake-level laws bind the repository, the portfolio-snapshot
identity, the basis head, the current main, the recorded issue, the
accepted contracts, the campaign references and the derived artifacts.
Each wrong identity is reported once with its exact route:

| law | route | intake-level trigger |
| --- | --- | --- |
| `repository_identity` | `recompile_basis` | intake names another repository |
| `basis_identity` | `recompile_basis` | intake binds another snapshot, a stale basis head or a wrong current main |
| `subject_identity` | `reconcile_selection` | the issue is not recorded in the captured issue source, the contract set is empty, or no derived artifacts |
| `campaign_ref_identity` | `reconcile_selection` | the placement or a relevant campaign is not a recorded campaign |
| `legacy_compatibility` | `reconcile_selection` | a legacy ref does not resolve read-only to a recorded campaign or the intake issue |

### Captured-input reuse

The corpus never duplicates captured bytes: each scenario names a
`fixtures/work_portfolio` corpus or a RIPR-SPEC-0244
`fixtures/work_selection_identity/captured/` directory. The
default-branch head is `7cb64d60c9dd0dda59586e6b72dcdf150296a26f` for
every referenced captured directory.

### Fixture provenance coverage

`fixtures/work_intake_identity/provenance.json` binds the sha256 of
`corpus.json`; `check-fixture-contracts` recomputes the digest in both
directions and reruns the check, so drifted or unlisted corpus bytes fail
closed.

### Legacy compatibility

`legacy_active_goal_ref` must resolve to a recorded campaign;
`legacy_current_work_item_ref` must match the intake issue identity. Both
emit migration posture, resolve read-only, and grant no write, readiness,
merge or closeout authority — a legacy active-goal packet attempting to
authorize intake fails closed.

## Required Evidence

- Committed corpus: five scenarios, eleven pinned cases, sha256-bound
  provenance, rerun by `check-fixture-contracts`.
- Unit tests: `xtask/src/work_intake_identity/tests.rs` (12 tests, all
  prefixed `work_intake_identity_`).
- Mutation-negative: the command writes only
  `target/ripr/reports/work-intake-check.{json,md}`.

## Inputs

- `fixtures/work_intake_identity/corpus.json`
- referenced captured portfolio directories (read-only)
- `fixtures/work_intake_identity/provenance.json`

## Outputs

- `target/ripr/reports/work-intake-check.json`
- `target/ripr/reports/work-intake-check.md`
- stdout (Markdown, or JSON with `--json`)

## Acceptance Examples

- An issue recorded in one campaign qualifies with placement and
  contracts (`intake-01`).
- An issue outside every campaign may still qualify (`intake-02`).
- An intake for review/repair of an existing open PR keeps the overlap
  visible (`intake-03`).
- A narrow one-file issue stays single-agent (`intake-04`).
- A legacy authority attempt, an unknown placement, an unknown campaign,
  an empty contract set, an unknown issue, a wrong repository and a wrong
  current main each fail with their exact law/route pair (`intake-05`).

## Non-Goals

- No intake places a campaign, selects work, claims a writer or grants
  readiness/merge/closeout authority.
- No candidate ranking; owned by #1704.
- No autonomous issue selection, agent spawning, source edit, merge or
  closeout.
- No planning-disposition or adapter consumer migrates in this slice.

## Test Mapping

- `work_intake_identity_committed_corpus_five_scenarios_hold`
- `work_intake_identity_in_campaign_qualified_passes`
- `work_intake_identity_out_of_campaign_still_qualifies`
- `work_intake_identity_overlap_visible_and_single_agent`
- `work_intake_identity_legacy_and_wrong_inputs_fail`
- `work_intake_identity_intake_schema_has_no_singleton_fields`
- `work_intake_identity_human_and_json_projections_agree`
- `work_intake_identity_deterministic_output`
- `work_intake_identity_corpus_loader_fails_closed`
- `work_intake_identity_committed_provenance_covers_every_corpus_byte`
- `work_intake_identity_intakes_bind_recorded_issues`
- `work_intake_identity_command_mutate_nothing_mutation_negative`

## Implementation Mapping

- `xtask/src/work_intake_identity.rs` — DTOs, corpus loader, intake law
  engine, runner, projections, command, fixture validator.
- `xtask/src/work_intake_identity/tests.rs` — the test mapping above.
- `xtask/src/work_selection_identity.rs` — the reused RIPR-SPEC-0244 law
  vocabulary and provenance DTO (referenced, never forked).
- `xtask/src/work_portfolio.rs` — the reused RIPR-SPEC-0234 compiler and
  captured-input loaders.
- `xtask/src/command.rs`, `xtask/src/dispatch.rs`, `xtask/src/main.rs`,
  `xtask/src/fixture_contracts/mod.rs`, `xtask/src/reports/fixtures.rs` —
  wiring and registrations.

## Metrics

- unit_test_pass_rate
- fixture corpus pass rate (pinned at 100% by `check-fixture-contracts`)
