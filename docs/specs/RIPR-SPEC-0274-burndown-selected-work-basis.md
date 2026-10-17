# RIPR-SPEC-0274: Burn-down binds the selected-work portfolio basis

Status: proposed

Owner: test-infra

Created: 2026-10-17

Linked issues:

- #1706 (parent: bind packets, issue lifecycle and burn-down to
  portfolio-selected work identity; this spec implements the #1648 consumer
  of delivery slice PR B — burn-down)
- #1648 (the burn-down consumer being migrated here)
- #6864 / RIPR-SPEC-0244 (the shared selected-work identity schemas this
  consumer reuses: `PortfolioBasisV1`, `SelectedWorkIdentityV1`, the sealed
  law/route vocabulary and the identity law engine; referenced, never
  duplicated)
- #1704 / #1794 / RIPR-SPEC-0234 (the work-portfolio compiler whose
  captured inputs the check corpus references)
- #1692 (goals-portfolio migration epic)

Support-tier impact:

- None.
- The burn-down check is an offline typed validation over committed
  captured bytes. It launches no agent, selects no work, claims nothing,
  authorizes nothing, calls no provider, reads no live GitHub state, and
  writes only `target/ripr/reports/work-burndown-check.{json,md}` plus
  stdout.

Policy impact:

- None. No new process, network or file-policy surface. The command
  registers in the command mutability catalog as `report_only`; its
  mutation-negative test pins the read-only boundary against the fixture
  tree.

## Problem

#1706 PR A (RIPR-SPEC-0244) defined the shared selected-work identity and
portfolio-basis contract but deliberately migrated no consumer. The #1648
burn-down train still treated campaign status as stronger than
issue/spec/PR/current-head evidence: without a migrated schema, one
campaign record could promote an issue to complete while its acceptance
rows stayed open, or a burn-down could be anchored to an old main head
while claiming current evidence. PR B must make issue completion derive
from issue/spec acceptance, implementation slices, PRs, exact-head
review/proof and current main evidence, must keep campaign/work-item
records as plan context only, and must fail stale/wrong input visibly with
the exact recompile/reconcile route.

## Behavior

`cargo xtask work burndown check [--corpus <dir>] [--json]` loads the
committed burn-down corpus (default `fixtures/work_burndown_identity`) and,
for each scenario, compiles the referenced captured portfolio directory
with the unchanged RIPR-SPEC-0234 compiler into a `PortfolioBasisV1`, then
runs the burn-down law engine over each committed `WorkBurnDownV1` row.
Human Markdown and JSON projections derive from one check DTO and agree on
scenario, case and pass/fail counts; neither can strengthen the other.

### WorkBurnDownV1

Every burn-down row binds:

- `burndown_id` — a stable burn-down identity string.
- `repository` — must equal the basis repository.
- `portfolio_snapshot` — the compiled snapshot identity; another snapshot
  is stale.
- `basis_sha` — the captured default-branch head; another head is stale.
- `selection` — the full `SelectedWorkIdentityV1` the burn-down tracks
  (the RIPR-SPEC-0244 schema already forbids repository-wide campaigns,
  agent assignment, CI wait state and progress percentages).
- `current_main_sha` — the current main the burn-down derives against; it
  must match the captured default-branch head.
- `issue_evidence` — rows over issue/spec acceptance, implementation
  slices, PRs and exact-head review/proof, each `acceptance_covered`,
  `acceptance_open`, `proof_current`, `proof_stale` or `merged`. A
  burn-down with no rows derives from nothing.
- `completion` — the derived completion: `complete`, `partial` or `open`.
  `Complete` is only honest when no evidence row is open or stale.
- `campaign_records` — campaign/work-item record references consulted as
  plan context; they grant no authority and cannot override evidence rows.
- `derived_from` — retained artifact references the burn-down derives
  from; an empty list is a claim, not a burn-down.
- `legacy_active_goal_ref` / `legacy_current_work_item_ref` — read-only
  compatibility references (see Legacy compatibility).

### Burn-down laws and exact routes

The embedded selection must satisfy every RIPR-SPEC-0244 law first; the
burn-down-level laws then bind the repository, the portfolio-snapshot
identity, the basis head, the current main head, the evidence rows, the
derived completion, the campaign-record context and the derived artifacts.
Each wrong identity is reported once with its exact route:

| law | route | burn-down-level trigger |
| --- | --- | --- |
| `repository_identity` | `recompile_basis` | burn-down names another repository |
| `basis_identity` | `recompile_basis` | burn-down binds another snapshot, a stale basis head or a wrong current main |
| `subject_identity` | `reconcile_selection` | no evidence rows, a `Complete` completion over an open/stale row, or no derived artifacts |
| `campaign_ref_identity` | `reconcile_selection` | a campaign record is neither a recorded campaign nor a campaign reference of the bound selection |
| `legacy_compatibility` | `reconcile_selection` | a legacy ref does not resolve read-only to the bound selection |

Embedded-selection violations fire through the shared engine with their
own exact routes.

### Captured-input reuse

The corpus never duplicates captured bytes: each scenario names a
`fixtures/work_portfolio` corpus or a RIPR-SPEC-0244
`fixtures/work_selection_identity/captured/` directory, and the packets
embed the committed RIPR-SPEC-0244 selection rows by deep copy at author
time (`author_corpus.py`). The default-branch head is
`7cb64d60c9dd0dda59586e6b72dcdf150296a26f` for every referenced captured
directory.

### Fixture provenance coverage

`fixtures/work_burndown_identity/provenance.json` binds the sha256 of
`corpus.json`; `check-fixture-contracts` recomputes the digest in both
directions and reruns the check, so drifted or unlisted corpus bytes fail
closed.

### Legacy compatibility

`legacy_active_goal_ref` must resolve to a recorded campaign carried as a
`historical` reference by the bound selection; `legacy_current_work_item_ref`
must match the bound selection's issue identity. Both emit migration
posture, resolve read-only, and grant no write, readiness, merge or
closeout authority.

## Required Evidence

- Committed corpus: five scenarios, nine pinned cases, sha256-bound
  provenance, rerun by `check-fixture-contracts`.
- Unit tests: `xtask/src/work_burndown_identity/tests.rs` (14 tests, all
  prefixed `work_burndown_identity_`).
- Mutation-negative: the command writes only
  `target/ripr/reports/work-burndown-check.{json,md}`.

## Inputs

- `fixtures/work_burndown_identity/corpus.json`
- referenced captured portfolio directories (read-only)
- `fixtures/work_burndown_identity/provenance.json`

## Outputs

- `target/ripr/reports/work-burndown-check.json`
- `target/ripr/reports/work-burndown-check.md`
- stdout (Markdown, or JSON with `--json`)

## Acceptance Examples

- A burn-down whose evidence rows are all covered, current or merged
  derives `complete` (`burndown-01`).
- A standalone selection derives an honest burn-down (`burndown-02`).
- Burn-down stays `partial` after one campaign work item merges while
  issue acceptance remains uncovered (`burndown-03`).
- A campaign-record override of open evidence, a wrong current main, an
  unknown campaign record, an evidence-free burn-down and a wrong
  repository each fail with their exact law/route pair (`burndown-04`).
- Legacy refs resolve read-only with migration posture and no authority
  (`burndown-05`).

## Non-Goals

- No burn-down closes an issue, merges a PR or grants closeout authority.
- No campaign/work-item record can promote completion beyond the evidence
  rows.
- No autonomous selection, campaign placement, agent spawning, source
  edit, merge or closeout.
- No intake, planning-disposition or adapter consumer migrates in this
  slice.

## Test Mapping

- `work_burndown_identity_committed_corpus_five_scenarios_hold`
- `work_burndown_identity_complete_evidence_burn_down_passes`
- `work_burndown_identity_standalone_burn_down_passes`
- `work_burndown_identity_partial_after_one_campaign_item_merges`
- `work_burndown_identity_campaign_override_and_wrong_inputs_fail`
- `work_burndown_identity_legacy_refs_read_only_no_authority`
- `work_burndown_identity_completion_is_a_closed_vocabulary`
- `work_burndown_identity_embedded_selection_reuses_shared_laws`
- `work_burndown_identity_burndown_schema_has_no_singleton_fields`
- `work_burndown_identity_human_and_json_projections_agree`
- `work_burndown_identity_deterministic_output`
- `work_burndown_identity_corpus_loader_fails_closed`
- `work_burndown_identity_committed_provenance_covers_every_corpus_byte`
- `work_burndown_identity_merged_item_keeps_open_acceptance_visible`
- `work_burndown_identity_command_mutate_nothing_mutation_negative`

## Implementation Mapping

- `xtask/src/work_burndown_identity.rs` — DTOs, corpus loader, burn-down
  law engine, runner, projections, command, fixture validator.
- `xtask/src/work_burndown_identity/tests.rs` — the test mapping above.
- `xtask/src/work_selection_identity.rs` — the reused RIPR-SPEC-0244 law
  engine and provenance DTO (referenced, never forked).
- `xtask/src/work_portfolio.rs` — the reused RIPR-SPEC-0234 compiler and
  captured-input loaders.
- `xtask/src/command.rs`, `xtask/src/dispatch.rs`, `xtask/src/main.rs`,
  `xtask/src/fixture_contracts/mod.rs`, `xtask/src/reports/fixtures.rs` —
  wiring and registrations.

## Metrics

- unit_test_pass_rate
- fixture corpus pass rate (pinned at 100% by `check-fixture-contracts`)
