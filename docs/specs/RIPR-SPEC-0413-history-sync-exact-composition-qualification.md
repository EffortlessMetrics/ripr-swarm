# RIPR-SPEC-0413: History-sync admission must qualify the exact composed tree

Status: proposed

Owner: CI / admission and release

Created: 2026-11-03

Linked issues:

- [#12437](https://github.com/EffortlessMetrics/ripr-swarm/issues/12437)
- [#12009](https://github.com/EffortlessMetrics/ripr-swarm/pull/12009)
- [#4838](https://github.com/EffortlessMetrics/ripr-swarm/issues/4838) (interim route retirement)

Linked PR: [#12486](https://github.com/EffortlessMetrics/ripr-swarm/pull/12486)

Support-tier impact: none. This governs CI qualification, not static analyzer or verifier claims.

Policy impact: register in `docs/spec-index.d/`, `policy/doc-artifacts.d/`, and
`.ripr/traceability.d/`. The exception is interim, with no settings, queue,
release or publication authority.

## Problem

A source-to-swarm history import can exceed the ordinary 160-file / 3,000-line
admission budget and the diff-scoped `ripr-pr` Rust-line budget. Refusing every
import prevents preservation of the public source history; unconditionally
bypassing PR evidence allows an unqualified, potentially broken composition.
A Markdown-only net diff can import history and must not take the docs-only
product-qualification shortcut.

## Behavior

### Subjects and authorization

- **S**: exact 40-hex source commit from the other allowlisted repository,
  reachable from its default branch.
- **B**: exact 40-hex declared destination base, an ancestor of H and live main;
  it must not already contain S.
- **H**: reviewed same-repository PR head. H may be a repair descendant of an
  actual source-history merge, not necessarily a two-parent commit itself.
- **G**: the exact tested GitHub merge composition with ordered parents
  [current base, H], whose tree and source reachability are checked. A
  historical PR synthetic merge or a changed base is not G.

The history-sync exception requires a `pull_request` request, an explicitly
selected `merge-ready` candidate, a matching `history-sync` label and exactly
one source and destination-base declaration in its PR body. The latest
history-sync label event must be an application by a current admin/maintain
actor. Removed/reapplied labels, unreadable paginated history, changed roles,
moved H/B/G or mismatched source declaration withdraw the exception. Current
authorization is re-read immediately before the terminal result. It is a
readback, not an atomic compare-and-swap or a replacement for protection.

Missing or contradictory evidence falls back to ordinary bounded admission
and, when oversized, refuses. It cannot turn an invalid sync into a normal
admitted oversized PR.

### Qualification

For a validated history sync, ordinary diff-size caps may be bypassed.
Exactly the diff-scoped PR evidence producers may report the typed
`not_applicable:history_sync:<H>` value, after exact G binding. All
non-diff-scoped obligations remain mandatory: compilation, fmt, Clippy, tests
and hosted shards, policy, goldens, schema, packages and applicable consumers.

Effective docs-only scope must be computed **after trusted admission**.
A validated history sync always runs a product implementation lane, even when
its net Git diff is Markdown-only. Docs-gate success alone cannot qualify it.
At least one selected implementation lane must actually succeed; every
succeeded implementation lane must carry exactly the bound typed record.
Missing, stale, wrong-head, wrong-subject, contradictory or skipped-required
evidence fails the required Result. Ordinary docs-only PRs retain the old
docs-only route; rejected and absent sync requests cannot force execution or
claim the typed exception.

The existing merge-queue path does not accept the interim history-sync
exception. Its historical two-parent queue subject cannot be misclassified
as a validated pull-request import. No administrative merge follows from
passing this policy. This route retires only after the protected
history-preserving imports and verified queue-only cutover.

## Non-Goals

The exception does not create an administrative merge path, a queue-only
history-sync profile, a source-promotion authorization, or release/publication
authority. Readback of a live base, head and actor is not an atomic expected-base
lock; those remain separate protected integration controls.

## Acceptance Examples

- Valid S/B/H ancestry and privileged latest label → admitted exact subject,
  with a single-parent repair descendant allowed.
- Missing source, foreign/fork head, wrong source branch, S already in B,
  moved head/base or unreadable graph → no history-sync exemption.
- Docs-only net diff plus validated sync → required product execution and
  exact typed result; all implementation lanes skipped → refusal.
- Ordinary docs-only PR → unchanged docs gate, with no typed sync claim.
- Wrong typed receipt or H, missing hosted shard, changed authorization event,
  changed actor permission, changed G parent/tree, or base movement → refusal.
- Native runner/permission API and branch-protection checks remain required
  hosted acceptance beyond controlled local fixtures.

## Test Mapping

- `candidate_admission.py::history_sync_self_test` tests the canonical
  source/base/head, label and ancestry admission.
- `test_history_sync_scope.py` executes the routed workflow's real Bash
  scope/result blocks with positive and falsifying negative inputs.
- `test_history_sync_live.py` executes the continuity helper against an
  injected API/validator and checks invalidation of event/actor/H/B/G.
- `xtask/tests/hard_cut_admission_selftest.rs` owns the Rust test entry points
  `history_sync_scope_real_bash_controls_pass` and
  `history_sync_live_authorization_controls_pass`. These run the controlled
  Bash/Python tests on Unix without network access.
- `xtask/tests/routed_dispatch_result.rs` executes the Result and PR-evidence
  workflow blocks against Git fixtures. It covers the exact-head typed skip,
  merge-group refusal, and bound history-sync admission in
  `history_sync_skip_qualifies_only_for_a_history_sync_admission_of_the_same_head`,
  `history_sync_skip_cannot_qualify_a_merge_group`, and
  `rust_gates_skips_pr_evidence_only_for_a_bound_history_sync_admission`.

## Implementation Mapping

- `.github/actions/pr-candidate-admission/candidate_admission.py`:
  canonical history-sync admission from the earlier candidate.
- `.github/actions/pr-candidate-admission/history_sync_live.py`:
  read-only final event/actor/graph/currentness witness.
- `.github/workflows/routed-rust.yml`: effective docs-only scope,
  selected implementation, exact typed PR-evidence and final Result.
- `.github/workflows/rust-gates.yml`: limit the exception to diff-scoped
  PR evidence while preserving the ordinary non-diff-scoped tests.

## Required Evidence

```bash
python3 .github/actions/pr-candidate-admission/candidate_admission.py --self-test
python3 .github/actions/pr-candidate-admission/test_history_sync_live.py -q
python3 .github/actions/pr-candidate-admission/test_history_sync_scope.py -q
cargo test -p xtask --test hard_cut_admission_selftest
cargo test -p xtask --test routed_dispatch_result
cargo test -p xtask --test rust_gate_workflow_contract
cargo xtask check-traceability
cargo xtask check-doc-artifacts
cargo xtask check-workflows
```

These fixtures do not establish that the deployed GitHub token can read
collaborator permissions, that live branch protection will reject movement
after readback, or that the final integration has been qualified. Those
remain hosted acceptance obligations; source-review readiness alone does not
qualify the PR.

## Metrics

- `unit_test_pass_rate` for the focused source/Result negative controls.
- Accepted history-sync subjects with an exact, live G-bound product lane.
- Required Result refusals for invalid label authority, source/base/head
  currentness, composition, typed evidence, or missing implementation proof.
- Ordinary PR route parity and the absence of diff-scoped evidence exceptions
  outside an admitted history-sync subject.
