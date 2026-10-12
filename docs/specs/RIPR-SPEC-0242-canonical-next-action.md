# RIPR-SPEC-0242: Canonical next action

Status: proposed

Owner: ripr-swarm

Created: 2026-10-05

Linked proposal:

Linked ADRs:

Linked plan:

Linked issues: #6304, #7257, #7258

Linked PRs:

Support-tier impact:

- No tier change. `docs/status/SUPPORT_TIERS.md` remains unchanged; this spec
  projects guidance over existing Rust surfaces and adds no language support,
  platform, or packaging claim.

Policy impact:

## Problem

First-hour surfaces (`ripr check` triage, the default RepairCard
`next_action`, RepairAttempt status, `ripr doctor` recovery, pilot
delegation) each choose a next action with their own prose and their own
strength rules. Nothing stops a renderer from emitting a stronger action
than the state supports: a best-effort command string for a route the
underlying authority would refuse, a ranked pick presented as a decision,
or a stale subject presented as current.

## Behavior

One shared selector, `select_canonical_next_action`, projects exactly one
`CanonicalNextActionV1` from normalized producer state:

- A closed action-class vocabulary of eight values: `run_command` (the only
  executable class), `inspect_details`, `choose_item`, `choose_attempt`,
  `satisfy_prerequisite`, `retry_current_subject`, `terminal_no_action`, and
  `unsupported_or_limited`.
- At most one primary action; alternatives are bounded (three) and
  subordinate detail references, never competing commands.
- Exact subject/currentness/root binding: the producer-bound root (never the
  process working directory), the exact diff-source mode (working tree vs
  committed), and one bound item — or no item exactly when the action is a
  selection among stop-carried candidates. Diff-source names the subject
  the action acts on: the analyzed diff for check replay (bound from the
  producer's declared provenance of the effective analysis source —
  explicit `--worktree` or the dirty-workspace default — never from the
  `--worktree` flag alone and never derived from base presence), the
  tree state the route reads for card and status.
- Typed prerequisites and stops instead of best-effort strings: check/card
  disagreement, stale head or config, route refusal, missing platform
  rendering, manual steps, terminal completion, and producer limitations
  each have a machine kind with exact identities and routes.
- Executability requires a producer-offered `CommandSpec` whose route admits
  the selected target on the current platform, with bound fresh head
  currentness. Surfaces that only have command lines (strings) can never
  yield `run_command`.
- Pilot names an action only through its delegated check/repair
  transaction; anything else is a typed limitation, never an invented route.

Human and JSON projections derive from the same normalized DTO and agree on
subject, class, command identity, reason, and limitations.

## Non-Goals

New repair lifecycle, stores, grammar, or ranking; top-level command
aliases; execution or source edits; transport rewrites; release decisions;
renderer migration for doctor/pilot/inventory surfaces (their producers are
covered by typed constructors and tests; owner follow-ups migrate the
renders).

Requirement-level v2 blocks and PR-local implementation slices belong in their
respective authorities; do not duplicate their normative prose here.
Maintenance-review metadata is optional and non-normative. Acceptance of this
document does not imply implementation, evidence, or support.

## Required Evidence

The DTO on the wire for card (`RepairCardV1.canonical_next_action`),
status (`agent_attempt_status.canonical_next_action`), and the public
navigation-aware `ripr check --format json` adapter; the
`next_action_class` governed-enum registration proving the vocabulary is
identical in code, registry, and docs; the control battery (controls 1-10)
with each gate owning one focused negative. The generic check JSON renderer
and unbounded `pr-evidence` path omit the object.

## Inputs

- `NextActionInput`: producer identity, producer-bound root, exact
  diff-source mode, bound item and check/card picks, unranked candidates,
  attempt views, head/config currentness, one offered `CommandSpec`,
  route admission, current platform, declared limitation, detail and
  restart routes, transition labels, producer-specific case handles
  (check triage case, doctor recovery, pilot delegation), and subordinate
  alternatives.

## Outputs

- `CanonicalNextActionV1` (`canonical_next_action.v1`): producer, subject,
  currentness, action class, optional command reference, optional typed stop,
  optional expected transition, bounded alternatives, limitations, and the
  standing non-claim.
- Human block and JSON document projections of the same DTO.

## Acceptance Examples

- A ready card with an admitted inspection route yields `run_command`
  referencing the offered spec verbatim; the card's `next_action` reference
  is `Some` with the same identity.
- A stale head yields `retry_current_subject` with a `refresh_currentness`
  stop naming the restart route, even when a command was offered.
- Check and card disagreeing on the selected item yield `choose_item` with a
  `resolve_disagreement` stop naming both picks; no silent choice.
- A terminal attempt yields `terminal_no_action` with receipt details and no
  continue command; read-only inspection specs stay available.
- Removing any one selector gate fails that gate's focused negative.

## Test Mapping

- `crates/ripr/src/domain/next_action.rs::tests::control_1_disagreement_refuses_rather_than_choosing`
- `crates/ripr/src/domain/next_action.rs::tests::control_10_each_gate_owns_its_negative`
- `crates/ripr/src/domain/next_action.rs::tests::check_cases_map_to_their_closed_classes`
- `crates/ripr/src/output/next_action.rs::tests::control_9_human_and_json_agree_from_one_dto`
- `crates/ripr/src/output/next_action.rs::tests::control_8_stopped_renders_omit_command_strings`
- `crates/ripr/src/app/repair_card.rs::tests::canonical_decision_and_reference_agree`
- `crates/ripr/src/app/agent_status.rs::tests::canonical_decision_selects_the_status_arm`
- `crates/ripr/src/output/human/triage.rs::tests::check_adapter_binds_the_ranked_winner_with_bounded_alternatives`
- `crates/ripr/src/output/human/triage.rs::tests::worktree_flag_selector_misattributes_dirty_default_as_committed_history`
- `crates/ripr/src/app/navigation.rs::tests::worktree_flag_alone_misattributes_a_dirty_default_run`
- `crates/ripr/tests/cli_smoke/next_action.rs::agent_card_json_embeds_the_canonical_decision`
- `crates/ripr/tests/cli_smoke/next_action.rs::default_dirty_check_binds_worktree_provenance_and_reopens_the_finding`
- `crates/ripr/src/output/render.rs::tests::navigation_aware_check_json_embeds_the_shared_producer_decision`
- `crates/ripr/src/output/render.rs::tests::restoring_the_generic_json_path_loses_the_canonical_action`
- `crates/ripr/src/output/render.rs::tests::findings_budget_does_not_change_the_primary_canonical_action`

## Implementation Mapping

- `crates/ripr/src/domain/next_action.rs`: closed vocabulary, typed stops,
  selection law, `select_canonical_next_action`.
- `crates/ripr/src/output/next_action.rs`: human and JSON projections.
- `crates/ripr/src/app/repair_card.rs`: card adapter; `next_action`
  projected from the canonical decision.
- `crates/ripr/src/app/agent_status.rs`: status adapter; arm dispatch from
  the canonical decision; `canonical_next_action` JSON key.
- `crates/ripr/src/output/human/triage.rs`: check adapter; line-family
  dispatch from the canonical case; diff-source bound from declared
  provenance of the effective analysis source; `detail_route` copied onto
  the `check_triage` stop.
- `crates/ripr/src/output/render.rs`: navigation-aware check JSON adapter
  embeds the same producer decision; generic JSON omits it.
- `crates/ripr/src/app/navigation.rs`: `CheckDiffProvenance::from_effective_source`
  for the check producer.
- `crates/ripr/src/cli/commands/check.rs`: binds provenance from `worktree_run`,
  not from the `--worktree` flag alone.
- `crates/ripr/src/cli/commands/agent_card.rs`: prose renders the canonical
  block with a legacy fallback.
- `crates/ripr/src/repair_card_digest.rs`: digest pins the portable
  decision parts.

## CI Proof

Which commands and CI lanes prove it?

- `cargo test -p ripr --lib -- next_action repair_card agent_status triage digest agent_card`
- `cargo xtask check-output-contracts`
- `cargo xtask check-public-api`
- `cargo xtask check-static-language`
- `cargo xtask check-traceability`
- `cargo xtask precommit`

## Metrics

What measurements show the behavior is working? Promotion decisions belong to
the applicable support and release authorities.

- Zero renderers minting a stronger action than the DTO decides (pinned by
  byte-identical renders plus decision-routing tests).
- Control battery green with each gate owning one focused negative.

## Failure Modes

- Producer binds no subject: the selector errors and the surface renders no
  action rather than guessing.
- Doctor or pilot without a precise recovery or delegation: typed
  limitation, never an invented command.
- Unknown host platform: commands fail closed to
  `unsupported_or_limited` with an explicit alternative route.
