# RIPR-SPEC-0272: Context/work-item packets bind the selected-work portfolio basis

Status: proposed

Owner: test-infra

Created: 2026-10-17

Linked issues:

- #1706 (parent: bind packets, issue lifecycle and burn-down to
  portfolio-selected work identity; this spec implements the #1632 consumer
  of delivery slice PR B — context/work-item packets)
- #1632 (the context/work-item packet consumer being migrated here)
- #6864 / RIPR-SPEC-0244 (the shared selected-work identity schemas this
  consumer reuses: `PortfolioBasisV1`, `SelectedWorkIdentityV1`, the sealed
  law/route vocabulary and the identity law engine; referenced, never
  duplicated)
- #1704 / #1794 / RIPR-SPEC-0234 (the work-portfolio compiler whose
  captured inputs the check corpus references)
- #1692 (goals-portfolio migration epic)

Support-tier impact:

- None.
- The context check is an offline typed validation over committed captured
  bytes. It launches no agent, selects no work, claims nothing, authorizes
  nothing, calls no provider, reads no live GitHub state, and writes only
  `target/ripr/reports/work-context-check.{json,md}` plus stdout.

Policy impact:

- None. No new process, network or file-policy surface. The command
  registers in the command mutability catalog as `report_only`; its
  mutation-negative test pins the read-only boundary against the fixture
  tree.

## Problem

#1706 PR A (RIPR-SPEC-0244) defined the shared selected-work identity and
portfolio-basis contract but deliberately migrated no consumer. The #1632
context/work-item packet train still assumed an `active goal` content
digest and one current work item: without a migrated schema, a packet
could still be compiled for another repository, an older portfolio
snapshot, a different issue or action, or a stale basis, and a packet
could smuggle in a repository-wide portfolio dump or let one campaign
pointer decide whose context counts. PR B must make every packet bind the
repository, the portfolio-snapshot identity, the basis head and the full
selected-work identity it compiles context for, must keep every carried
reference relevant to that selection, and must fail stale/wrong input
visibly with the exact recompile/reconcile route.

## Behavior

`cargo xtask work context check [--corpus <dir>] [--json]` loads the
committed context-packet corpus (default `fixtures/work_context_identity`)
and, for each scenario, compiles the referenced captured portfolio
directory with the unchanged RIPR-SPEC-0234 compiler into a
`PortfolioBasisV1`, then runs the packet law engine over each committed
`WorkContextPacketV1` row. Human Markdown and JSON projections derive from
one check DTO and agree on scenario, case and pass/fail counts; neither
can strengthen the other.

### WorkContextPacketV1

Every packet row binds:

- `packet_id` — a stable packet identity string.
- `repository` — must equal the basis repository.
- `portfolio_snapshot` — the compiled snapshot identity; another snapshot
  is stale.
- `basis_sha` — the captured default-branch head; another head is stale.
- `selection` — the full `SelectedWorkIdentityV1` the packet compiles
  context for (the RIPR-SPEC-0244 schema already forbids repository-wide
  campaigns, agent assignment, CI wait state and progress percentages).
- `scope` — a closed vocabulary: `selected_work_only` or
  `selected_work_plus_relevant_overflow`. There is deliberately no
  repository-wide scope.
- `context_refs` — relevant campaign/spec/context references. Every entry
  must name a campaign reference's id or source, an accepted requirement,
  a spec ref, an implementation slice, or an overlap issue/PR/claim/
  worktree identity of the bound selection.
- `overflow_refs` — relevant overflow for the widened scope; each entry
  must satisfy the same relevance rule. A `selected_work_only` packet
  must not carry overflow.
- `compiled_from` — retained artifact references the packet was compiled
  from; an empty list is a claim, not a packet.
- `legacy_active_goal_ref` / `legacy_current_work_item_ref` — read-only
  compatibility references (see Legacy compatibility).

A packet may select an issue with no campaign placement at all; campaign
references are context, never permission.

### Packet laws and exact routes

The embedded selection must satisfy every RIPR-SPEC-0244 law first; the
packet-level laws then bind the repository, the portfolio-snapshot
identity, the basis head, the compiled artifacts, the bounded scope and
the relevance of every carried reference. Each wrong identity is reported
once with its exact route:

| law | route | packet-level trigger |
| --- | --- | --- |
| `repository_identity` | `recompile_basis` | packet names another repository |
| `basis_identity` | `recompile_basis` | packet binds another snapshot or a stale basis head |
| `subject_identity` | `reconcile_selection` | packet compiled from nothing, or a selected-work-only packet carries overflow |
| `campaign_ref_identity` | `reconcile_selection` | a carried context/overflow reference is not relevant to the bound selection |
| `legacy_compatibility` | `reconcile_selection` | a legacy ref does not resolve read-only to the bound selection |

Embedded-selection violations (subject, action, basis, head, worktree,
campaign-ref, overlap and legacy laws) fire through the shared engine
with their own exact routes.

### Captured-input reuse

The corpus never duplicates captured bytes: each scenario names a
`fixtures/work_portfolio` corpus or a RIPR-SPEC-0244
`fixtures/work_selection_identity/captured/` directory, and the packets
embed the committed RIPR-SPEC-0244 selection rows by deep copy at author
time (`author_corpus.py`). The default-branch head is
`7cb64d60c9dd0dda59586e6b72dcdf150296a26f` for every referenced captured
directory.

### Fixture provenance coverage

`fixtures/work_context_identity/provenance.json` binds the sha256 of
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

- Committed corpus: six scenarios, eleven pinned cases, sha256-bound
  provenance, rerun by `check-fixture-contracts`.
- Unit tests: `xtask/src/work_context_identity/tests.rs` (13 tests, all
  prefixed `work_context_identity_`).
- Mutation-negative: the command writes only
  `target/ripr/reports/work-context-check.{json,md}`.

## Inputs

- `fixtures/work_context_identity/corpus.json`
- referenced captured portfolio directories (read-only)
- `fixtures/work_context_identity/provenance.json`

## Outputs

- `target/ripr/reports/work-context-check.json`
- `target/ripr/reports/work-context-check.md`
- stdout (Markdown, or JSON with `--json`)

## Acceptance Examples

- A packet for a one-campaign selection compiles and passes
  (`context-01`).
- Several related campaign references stay context, never permission
  (`context-02`).
- A standalone issue with no campaign placement compiles a valid packet
  (`context-03`).
- A repair packet binds the recorded PR, claim and worktree as relevant
  overflow (`context-04`).
- A narrow one-file issue stays single-agent despite active campaigns
  (`context-05`).
- An unresolvable legacy active-goal ref, an irrelevant carried reference,
  a selected-work-only packet with overflow, an evidence-free packet, a
  wrong repository and a stale basis head each fail with their exact
  law/route pair (`context-06`).

## Non-Goals

- No packet compiles context for more than one selected transition.
- No packet carries raw portfolio, issue or log dumps; references only.
- No packet grants write, readiness, merge or closeout authority.
- No autonomous selection, campaign placement, agent spawning, source
  edit, merge or closeout.
- No synthesis, intake, planning-disposition, burn-down or adapter
  consumer migrates in this slice.

## Test Mapping

- `work_context_identity_committed_corpus_six_scenarios_hold`
- `work_context_identity_valid_packets_bind_their_selections`
- `work_context_identity_campaign_refs_are_context_not_permission`
- `work_context_identity_legacy_refs_read_only_no_authority`
- `work_context_identity_irrelevant_refs_and_unbounded_scope_fail`
- `work_context_identity_wrong_repository_and_stale_basis_fail`
- `work_context_identity_embedded_selection_reuses_shared_laws`
- `work_context_identity_packet_schema_has_no_singleton_fields`
- `work_context_identity_human_and_json_projections_agree`
- `work_context_identity_deterministic_output`
- `work_context_identity_corpus_loader_fails_closed`
- `work_context_identity_committed_provenance_covers_every_corpus_byte`
- `work_context_identity_scope_is_a_closed_bounded_vocabulary`
- `work_context_identity_command_mutate_nothing_mutation_negative`

## Implementation Mapping

- `xtask/src/work_context_identity.rs` — DTOs, corpus loader, packet law
  engine, runner, projections, command, fixture validator.
- `xtask/src/work_context_identity/tests.rs` — the test mapping above.
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
