# RIPR-SPEC-0235: Shared selected-work identity schemas and legacy compatibility

Status: proposed

Owner: test-infra

Created: 2026-10-05

Linked issues:

- #1706 (parent: bind packets, issue lifecycle and burn-down to
  portfolio-selected work identity; this spec implements delivery slice PR A
  of 4 — shared identity schemas and compatibility, no consumer rewrite)
- #1704 / #1794 / RIPR-SPEC-0234 (the work-portfolio compiler whose
  repository identity, source-observation, confidence and candidate-kind
  DTOs this contract reuses; referenced, never duplicated)
- #1692 (goals-portfolio migration epic)
- #4677 / #4678 / #4679 (downstream delivery slices: context/task/plan/claim/
  synthesis consumers, issue-lifecycle and burn-down binding, and
  singleton-path retirement; they consume this vocabulary, implemented
  there)
- #1646 (the closed plan-disposition vocabulary frozen here for the PR C
  planning consumer)
- RIPR-SPEC-0218 / RIPR-SPEC-0223 (the lifecycle disposition and
  provenance/digest conventions this contract reuses)

Support-tier impact:

- None. The identity check is an offline typed validation over committed
  captured bytes. It launches no agent, selects no work, claims nothing,
  authorizes nothing, calls no provider, reads no live GitHub state, and
  writes only `target/ripr/reports/work-selection-check.{json,md}` plus
  stdout.

Policy impact:

- None. No new process, network or file-policy surface. The command
  registers in the command mutability catalog as `report_only`; its
  mutation-negative test pins the read-only boundary against the fixture
  tree and source files.

## Problem

#1704 removed the singleton active-goal authority and produced a
deterministic multi-campaign portfolio, but no shared contract yet binds a
root-selected transition to that portfolio. Without one, the #1632–#1637
packet/train consumers and the #1644–#1649 issue-lifecycle consumers would
each re-invent identity fields, and the migration could silently recreate
the singleton: a packet could require an active-goal digest, triage could
prefer one campaign pointer, planning could replace one global campaign, a
generic active-goal resource could serialize unrelated work, burn-down could
treat campaign status as stronger than issue/spec/PR/current-head evidence,
and legacy `active.toml`-era references could keep granting authority.
#1706 PR A must define the shared identity DTOs, the normalized identity
rules, the legacy compatibility states and the captured fixture corpus
before any consumer migration lands.

## Behavior

`cargo xtask work selection check [--corpus <dir>] [--json]` loads the
committed selection corpus (default `fixtures/work_selection_identity`) and,
for each scenario, compiles the referenced captured portfolio directory with
the unchanged RIPR-SPEC-0234 compiler into a `PortfolioBasisV1`, then runs
the identity law engine over each committed `SelectedWorkIdentityV1` packet.
Human Markdown and JSON projections derive from one check DTO and agree on
scenario, case and pass/fail counts; neither can strengthen the other.

### PortfolioBasis

`PortfolioBasisV1` binds, reusing the #1704 compiler DTOs rather than
forking them: repository and default-branch identity
(`WorkRepositoryIdentityV1`), the compiled snapshot's portable identity
digest, snapshot completeness (`WorkConfidenceV1`, `Complete` only when
every captured source is current), every captured source observation
(`WorkSourceObservationV1`), the unavailable source names, and the selection
policy plus version.

### SelectedWorkIdentity

`SelectedWorkIdentityV1` binds exactly one root-selected transition: the
selection id, repository, candidate id, issue / work-item / pull-request
subject identities in stable string forms (`issue:<n>`, `work-item:<id>`,
`candidate:issue:<n>`), the selected lifecycle action (the closed
RIPR-SPEC-0234 candidate-kind vocabulary, not a parallel taxonomy),
accepted requirement/spec/implementation-slice references, zero or more
`RelevantCampaignRefV1` campaign references (`member` | `relevant` |
`historical`, each source-linked), the basis SHA and expected head, the
`LiveOverlapSetV1` of overlapping issues/PRs/claims/worktrees/resources, the
role/context/budget profile (including the fixed single-agent flag), the
claim boundary and the stop conditions. The schema has no repository-wide
current/default campaign, no mutable agent assignment, no CI wait state and
no progress percentage; `deny_unknown_fields` rejects any such addition.

### Identity laws and exact routes

Every law violation carries its law and its exact recovery route: wrong
repository or stale basis SHA routes `recompile_basis`; wrong issue,
work-item, pull request, action, campaign reference or hidden overlap
routes `reconcile_selection`; an unknown claimed worktree routes
`reconcile_resources`; a mismatched expected head routes `reconcile_head`.
Overlap visibility requires every live PR, claim, worktree and semantic
resource the portfolio shows for the selected issue to be recorded in the
packet before any mutation. Campaign references are context only: zero
campaigns is valid standalone work, and no reference grants execution
authority.

### Legacy compatibility

`legacy_active_goal_ref` and `legacy_current_work_item_ref` are accepted
only through explicit schema compatibility. They resolve to recorded
campaign references carried as `historical` relations, emit migration
posture recording that they grant no write, readiness, merge or closeout
authority, and a legacy packet whose `attempted_authorities` is non-empty
fails with `legacy_compatibility`/`reject_legacy_authority`. Campaign
records carry durable intent only; they cannot promote readiness, proof,
support or completion.

### Frozen plan-disposition vocabulary

`WorkPlanDispositionV1` freezes the eight #1646 wire names for the PR C
planning consumer: `single_scoped_pr`, `multi_pr_campaign`,
`append_to_named_campaign`, `standalone_issue_work`, `focused_tracker_only`,
`already_planned`, `blocked_by_contract_or_decision`,
`root_portfolio_decision_required`. The corpus pins each scenario's
disposition at schema/fixture level; PR C consumes the enum.

## Required Evidence

- The committed corpus `fixtures/work_selection_identity/corpus.json` with
  all twelve #1706 required scenarios and per-case pinned expectations.
- `fixtures/work_selection_identity/provenance.json` SHA-256 bindings,
  recomputed fail-closed by `cargo xtask check-fixture-contracts`.
- `cargo xtask work selection check` passing on the committed corpus, with
  byte-stable JSON output and a stable portable identity across runs.
- `cargo test -p xtask work_selection_identity` pinning the acceptance
  items PR A can honestly pin.

## Inputs

- The committed selection corpus (`corpus.json`) and the captured portfolio
  directories it references under `fixtures/`.

## Outputs

- `work_selection_check_view.v1` JSON and the derived Markdown on stdout and
  under `target/ripr/reports/work-selection-check.{json,md}`.

## Acceptance Examples

- A packet selecting issue `9105` for `start_build` against the canonical
  corpus passes with one member campaign reference and no violations.
- The same packet against the stale-local variant still passes — the
  snapshot identity moved, the basis head did not — while snapshot
  completeness degrades to `partial`.
- A packet that does not record the newly introduced open PR `8899` fails
  with `overlap_visibility`/`reconcile_selection` naming PR `8899`.
- A legacy packet with `attempted_authorities: ["writer"]` fails with
  `legacy_compatibility`/`reject_legacy_authority`.
- Wrong repository, issue, action, basis, worktree and head packets each
  fail with the exact law/route pairs pinned in scenario 9.

## Non-Goals

- No consumer migration: packets, tasks/results, plans, claims, synthesis,
  issue intake and burn-down keep their current behavior; PR B/PR C consume
  this vocabulary.
- No portfolio candidate ranking (owned by #1704), no autonomous selection,
  no agent spawning, no source edit, merge or closeout authority.
- No execution-wave DTO: wave planning consumes this identity in a later
  slice.
- No live GitHub, label, issue-age or title-keyword input; committed
  captured bytes are the only authority.

Requirement-level v2 blocks and PR-local implementation slices belong in
their respective authorities; do not duplicate their normative prose here.
Acceptance of this document does not imply implementation, evidence, or
support.

## Test Mapping

- `xtask/src/work_selection_identity.rs::tests::work_selection_identity_committed_corpus_twelve_scenarios_hold`
- `xtask/src/work_selection_identity.rs::tests::work_selection_identity_wrong_identities_fail_visibly_with_exact_routes`
- `xtask/src/work_selection_identity.rs::tests::work_selection_identity_snapshot_change_keeps_compatible_issue_head`
- `xtask/src/work_selection_identity.rs::tests::work_selection_identity_new_overlap_forces_reconcile`
- `xtask/src/work_selection_identity.rs::tests::work_selection_identity_legacy_refs_read_only_no_authority`
- `xtask/src/work_selection_identity.rs::tests::work_selection_identity_campaign_representations_without_default`
- `xtask/src/work_selection_identity.rs::tests::work_selection_identity_no_singleton_or_runtime_state_fields`
- `xtask/src/work_selection_identity.rs::tests::work_selection_identity_human_and_json_projections_agree`
- `xtask/src/work_selection_identity.rs::tests::work_selection_identity_plan_disposition_vocabulary_frozen`
- `xtask/src/work_selection_identity.rs::tests::work_selection_identity_stable_forms_and_equality`
- `xtask/src/work_selection_identity.rs::tests::work_selection_identity_burndown_partial_projection_states`
- `xtask/src/work_selection_identity.rs::tests::work_selection_identity_fresh_root_resume_from_artifacts_only`
- `xtask/src/work_selection_identity.rs::tests::work_selection_identity_deterministic_output`
- `xtask/src/work_selection_identity.rs::tests::work_selection_identity_corpus_loader_fails_closed`
- `xtask/src/work_selection_identity.rs::tests::work_selection_identity_provenance_fails_closed`
- `xtask/src/work_selection_identity.rs::tests::work_selection_identity_command_mutate_nothing_mutation_negative`
- `xtask/src/work_selection_identity.rs::tests::work_selection_identity_basis_completeness_reflects_sources`

## Implementation Mapping

- `xtask/src/work_selection_identity.rs` — DTOs, basis construction, the
  identity law engine, corpus runner, projections, command and fixture
  validator.
- `xtask/src/work_portfolio.rs` — shared identity digest helper, workspace
  path helper and candidate-kind wire names widened to `pub(crate)`; the
  compiler itself is unchanged.
- `xtask/src/command.rs`, `xtask/src/dispatch.rs` — `work selection check`
  registration in the parser, dispatcher and the command catalog.
- `xtask/src/fixture_contracts/mod.rs`, `xtask/src/reports/fixtures.rs` —
  corpus validator registration and the manifest-only fixture exemption.

## Metrics

- `unit_test_pass_rate` over the `work_selection_identity` test prefix.
- Corpus expectation coverage: 12/12 scenarios, 18/18 pinned cases passing.
