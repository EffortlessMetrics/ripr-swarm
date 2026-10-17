# RIPR-SPEC-0273: Root synthesis binds the selected-work portfolio basis

Status: proposed

Owner: test-infra

Created: 2026-10-17

Linked issues:

- #1706 (parent: bind packets, issue lifecycle and burn-down to
  portfolio-selected work identity; this spec implements the #1636 consumer
  of delivery slice PR B — root synthesis)
- #1636 (the root synthesis consumer being migrated here)
- #6864 / RIPR-SPEC-0244 (the shared selected-work identity schemas this
  consumer reuses: `PortfolioBasisV1`, `SelectedWorkIdentityV1`, the sealed
  law/route vocabulary and the identity law engine; referenced, never
  duplicated)
- #1704 / #1794 / RIPR-SPEC-0234 (the work-portfolio compiler whose
  captured inputs the check corpus references)
- #1692 (goals-portfolio migration epic)

Support-tier impact:

- None.
- The synthesis check is an offline typed validation over committed
  captured bytes. It launches no agent, selects no work, claims nothing,
  authorizes nothing, calls no provider, reads no live GitHub state, and
  writes only `target/ripr/reports/work-synthesis-check.{json,md}` plus
  stdout.

Policy impact:

- None. No new process, network or file-policy surface. The command
  registers in the command mutability catalog as `report_only`; its
  mutation-negative test pins the read-only boundary against the fixture
  tree.

## Problem

#1706 PR A (RIPR-SPEC-0244) defined the shared selected-work identity and
portfolio-basis contract but deliberately migrated no consumer. The #1636
root synthesis train still reported against one current work item: without
a migrated schema, a synthesis could be produced for another repository,
an older portfolio snapshot, a different issue or action, or a stale
basis, could silently report a different transition than the one selected,
or could hide a coverage gap behind green rows. PR B must make every
synthesis bind the repository, the portfolio-snapshot identity, the basis
head and the full selected-work identity it reports on, must keep the
reported transition identical to the selected action, must surface every
recorded gap as a named contradiction, and must fail stale/wrong input
visibly with the exact recompile/reconcile route.

## Behavior

`cargo xtask work synthesis check [--corpus <dir>] [--json]` loads the
committed root-synthesis corpus (default `fixtures/work_synthesis_identity`)
and, for each scenario, compiles the referenced captured portfolio
directory with the unchanged RIPR-SPEC-0234 compiler into a
`PortfolioBasisV1`, then runs the synthesis law engine over each committed
`WorkSynthesisV1` row. Human Markdown and JSON projections derive from
one check DTO and agree on scenario, case and pass/fail counts; neither
can strengthen the other.

### WorkSynthesisV1

Every synthesis row binds:

- `synthesis_id` — a stable synthesis identity string.
- `repository` — must equal the basis repository.
- `portfolio_snapshot` — the compiled snapshot identity; another snapshot
  is stale.
- `basis_sha` — the captured default-branch head; another head is stale.
- `selection` — the full `SelectedWorkIdentityV1` the synthesis reports
  on (the RIPR-SPEC-0244 schema already forbids repository-wide
  campaigns, agent assignment, CI wait state and progress percentages).
- `reported_transition` — the selected lifecycle action reported; it must
  match the bound selection's action, so a synthesis cannot silently
  report a different transition than the one selected.
- `acceptance_coverage` — rows over the bound selection's accepted
  requirements and spec refs, each `covered`, `uncovered` or
  `contradicted`. A synthesis with no rows reports nothing.
- `contradictions` — every `uncovered` or `contradicted` row must be
  named here, so a synthesis can never hide a gap.
- `next_action` — the bounded next step for the already-selected work;
  never an encoded new selection.
- `derived_from` — retained artifact references the synthesis derives
  from; an empty list is a claim, not a synthesis.
- `reported_campaign_ref` — when present, must be one of the bound
  selection's campaign references; a synthesis cannot silently switch
  campaign.
- `legacy_active_goal_ref` / `legacy_current_work_item_ref` — read-only
  compatibility references (see Legacy compatibility).

The schema deliberately has no field that could create another selected
issue; `deny_unknown_fields` rejects such additions at parse time.

### Synthesis laws and exact routes

The embedded selection must satisfy every RIPR-SPEC-0244 law first; the
synthesis-level laws then bind the repository, the portfolio-snapshot
identity, the basis head, the reported transition, the coverage rows, the
contradiction honesty, the bounded next action, the derived artifacts and
the reported campaign reference. Each wrong identity is reported once with
its exact route:

| law | route | synthesis-level trigger |
| --- | --- | --- |
| `repository_identity` | `recompile_basis` | synthesis names another repository |
| `basis_identity` | `recompile_basis` | synthesis binds another snapshot or a stale basis head |
| `action_identity` | `reconcile_selection` | reported transition differs from the selected action |
| `subject_identity` | `reconcile_selection` | no coverage rows, a hidden uncovered/contradicted row, an empty next action or no derived artifacts |
| `campaign_ref_identity` | `reconcile_selection` | the reported campaign ref is not one of the bound selection's campaign refs |
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

`fixtures/work_synthesis_identity/provenance.json` binds the sha256 of
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

- Committed corpus: five scenarios, eight pinned cases, sha256-bound
  provenance, rerun by `check-fixture-contracts`.
- Unit tests: `xtask/src/work_synthesis_identity/tests.rs` (14 tests, all
  prefixed `work_synthesis_identity_`).
- Mutation-negative: the command writes only
  `target/ripr/reports/work-synthesis-check.{json,md}`.

## Inputs

- `fixtures/work_synthesis_identity/corpus.json`
- referenced captured portfolio directories (read-only)
- `fixtures/work_synthesis_identity/provenance.json`

## Outputs

- `target/ripr/reports/work-synthesis-check.json`
- `target/ripr/reports/work-synthesis-check.md`
- stdout (Markdown, or JSON with `--json`)

## Acceptance Examples

- A synthesis for a covered selected transition passes
  (`synthesis-01`).
- A standalone selection with zero campaign placement supports an honest
  synthesis (`synthesis-02`).
- Partial coverage after one campaign work item merges surfaces a named
  contradiction (`synthesis-03`).
- A hidden gap, a silent campaign switch, a mismatched transition, an
  evidence-free synthesis and a wrong repository each fail with their
  exact law/route pair (`synthesis-04`).
- Legacy refs resolve read-only with migration posture and no authority
  (`synthesis-05`).

## Non-Goals

- A synthesis creates no selected issue, switches no campaign and grants
  no authority.
- No acceptance-row status beyond the closed `covered` / `uncovered` /
  `contradicted` vocabulary.
- No autonomous selection, campaign placement, agent spawning, source
  edit, merge or closeout.
- No intake, planning-disposition, burn-down or adapter consumer
  migrates in this slice.

## Test Mapping

- `work_synthesis_identity_committed_corpus_five_scenarios_hold`
- `work_synthesis_identity_covered_transition_synthesis_passes`
- `work_synthesis_identity_standalone_synthesis_passes`
- `work_synthesis_identity_partial_coverage_contradiction_surfaced`
- `work_synthesis_identity_hidden_gap_silent_switch_and_wrong_inputs_fail`
- `work_synthesis_identity_legacy_refs_read_only_no_authority`
- `work_synthesis_identity_schema_creates_no_selection`
- `work_synthesis_identity_embedded_selection_reuses_shared_laws`
- `work_synthesis_identity_human_and_json_projections_agree`
- `work_synthesis_identity_deterministic_output`
- `work_synthesis_identity_corpus_loader_fails_closed`
- `work_synthesis_identity_committed_provenance_covers_every_corpus_byte`
- `work_synthesis_identity_contradiction_rows_must_be_named`
- `work_synthesis_identity_command_mutate_nothing_mutation_negative`

## Implementation Mapping

- `xtask/src/work_synthesis_identity.rs` — DTOs, corpus loader, synthesis
  law engine, runner, projections, command, fixture validator.
- `xtask/src/work_synthesis_identity/tests.rs` — the test mapping above.
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
