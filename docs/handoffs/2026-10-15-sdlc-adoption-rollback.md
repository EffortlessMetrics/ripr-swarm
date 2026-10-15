# SDLC candidates: adoption and rollback checklist

Observed 2026-10-15 UTC. **Source evidence is usable; adoption remains held.**
This is a bounded handoff, not a work selector or release receipt. Steven's
coordinating delivery thread owns integration and the final current-state decision.
No shared guide, owner candidate, label, workflow or protection is changed here.

## Exact inputs and continuation owners

| Candidate / existing owner | Immutable subject | Verified evidence / boundary |
| --- | --- | --- |
| Docs integration, `docs/integrate-authority-8008-8009-8020`; #8001 / #8009 / #8020 documentation lane | [`5d6d6c96d8574df59db0d3f86851736fd2f710a9`](https://github.com/EffortlessMetrics/ripr-swarm/commit/5d6d6c96d8574df59db0d3f86851736fd2f710a9), tree `d64466d6ee290610e175e1aec4259afe60824908`, parent `8b7a9ba4a7ca2f1212fc3b456d0a2bee876a5e55` | [Independent integration review](https://github.com/EffortlessMetrics/ripr-swarm/issues/8020#issuecomment-6183767282): complete 12-path review, native doc checks, six-to-zero index mismatches and eight-to-zero role omissions. REVIEW_INCOMPLETE for hosted integration. |
| Late-CI owner `codex/4987-windows-subset-admission` at `4eebbc6e6c3c078170790d52bca5877a0bda6d54`; companion `fix/4987-reject-empty-windows-evidence` | [`6902a132eb0366a31bdd1b52a30b189847c1f82f`](https://github.com/EffortlessMetrics/ripr-swarm/commit/6902a132eb0366a31bdd1b52a30b189847c1f82f), tree `2ed77d544fa296236d65502c6540c696b6cf32a6`; owner integration basis `16190da1b` | [Independent bounded review](https://github.com/EffortlessMetrics/ripr-swarm/issues/4987#issuecomment-6183728279): 406 controlled candidate executions, ten predecessor false successes and one mutation witness; Linux permanent tests 6 + 7. No actionable scoped source finding; native qualification incomplete. |
| Advisory reuse, `fix/evidence-source-equivalence`; [#8024](https://github.com/EffortlessMetrics/ripr-swarm/issues/8024) producer/summary lane | [`70136f5304423e902568388b7bb48cb4c97bc34f`](https://github.com/EffortlessMetrics/ripr-swarm/commit/70136f5304423e902568388b7bb48cb4c97bc34f), tree `9d9dbc374afddfed9c866086d511a511f9fa5115`, base `322088b1760388fb7625d48897abe8e611cb9715` | [Owner receipt](https://github.com/EffortlessMetrics/ripr-swarm/issues/8024#issuecomment-6182955533): final-head 45 proof tests, check-fast, precommit, process policy and actual CLI refusal controls; reported independent source review. Those executions were not repeated by this handoff. |

Remote `git ls-remote` readback at 03:56 UTC matched all four branches above.
All records are authored by GitHub account `EffortlessSteven`; this does not
establish a unique human/agent writer. The named branches/issues retain custody.
[#8008](https://github.com/EffortlessMetrics/ripr-swarm/pull/8008) remains a separate
Draft at `57d6d72f864d7b1705f95bcec7052ef854e74c9d`; do not alter it to force adoption.
#8009 and #8020 are issues, not PRs, despite the integration commit's shorthand.

## Adoption checklist — coordinating delivery owner

- [ ] Re-read current instructions, branch heads, owner decisions and actual
  integration subject. This handoff starts at main
  `7fb05f24d4d3eef9773044b237746e8cc3fbe972`; it does not qualify a future main.
  Preserve the explicit no-PR/no-live-request/no-cutover holds in the
  [late-CI candidate handoff](https://github.com/EffortlessMetrics/ripr-swarm/blob/6902a132eb0366a31bdd1b52a30b189847c1f82f/docs/handoffs/2026-10-15-windows-subset-admission.md).
  Opening even a Draft currently launches unrelated qualification, including
  Security. Branch publication alone is distinct from PR admission.
- [ ] Retain the fresh **`pull_request: labeled` / `merge-ready`** design from
  `6902a132`. Opening, pushing and Ready alone are not requests in that candidate.
  Unrelated labels get `Merge-ready Nonrequest`, isolated metadata concurrency
  and no protected-result impersonation. Older Ready-only / merge-group prose
  in #4987's history or current-main guides cannot replace this candidate design.
  Main still has its older routing: staged source is not activation.
- [ ] #4987 owner composes the evidence companion with its existing candidate;
  keep the frozen prototype inactive. Re-review any changed caller, callee,
  policy and instruction generation together. Admission and final refresh must
  retain old outstanding CHANGES_REQUESTED until native approval/dismissal;
  comments cannot clear it. Positive evidence remains current-head-bound.
- [ ] At an explicitly admitted rehearsal, #4986 admission / #4836 product
  owners verify actual API/token responses, paginated reviews/threads,
  H (head), B (base), T (integration commit), parents/tree, workflow generation,
  run/attempt and protected `Ripr Rust Small Result` association. Observe unrelated
  labels and Ready without routed qualification, then one explicit request.
  Check existing queued/running custody before requesting again.
- [ ] #4987 Windows owner executes one real reusable T-subject producer:
  PowerShell selector build/replay, real test/Clippy commands, nonempty canonical
  status/logs, selector receipt agreement and both cleanup outcomes. Valid
  nonzero subset verdicts stay advisory; green aggregation is not all Windows
  tests passing. Independently challenge missing/stale evidence, readiness drift,
  cancellation after successful dependencies/during finalization, and sequential
  duplicate API reuse. Mocked runs do not prove native delivery or scheduling.
  The [later test-only review](https://github.com/EffortlessMetrics/ripr-swarm/issues/4987#issuecomment-6184051444)
  also leaves raw streamed review-page arrays and tied `submitted_at` ordering
  unproved: existing fixtures use one array and distinct timestamps. Its executor
  failed provisioning; zero tests ran. This is a coverage gap, not an established
  production defect or a replacement for the successful evidence-integrity review.
- [ ] #4987 retains full portfolio ownership; #7827 owns cache continuity,
  #4841 economics, #4988 runner capacity and #4836 actual integration evidence.
  Account for other platform/feature/package/security obligations and unknown
  organization/provider settings. No graph-wide zero-launch, native queue,
  exactly-once scheduling or universal <=20-minute result is established.
- [ ] Docs owner adopts only the reviewed nine documents and three fragments;
  preserve SPEC_FORMAT disposition authority, meaningful role content and
  historical qualifiers. Accepted specs do not imply implementation, evidence,
  support or release readiness. [#8002](https://github.com/EffortlessMetrics/ripr-swarm/issues/8002)
  validator work remains separately owned.
- [ ] #8024 owner preserves original execution SHA/outcome for compatible
  source-tree/base/merge-base/plan/environment reuse. Changed context, legacy or
  malformed receipts and selected/executed mismatch refuse reuse. This is local
  advisory compatibility, not dependency authentication or protected CI proof.
  A docs integration changes the tree: assess compatibility; do not relabel old
  execution as a fresh combined-head run.
- [ ] Only after admitted qualification: read actual checkout/check results,
  current reviews/threads, required contexts and applicable protections; refresh
  affected proof and independent review, then normal protected squash integration
  and landed-tree readback. Required-result app15368/no-queue observation is
  [parent-reported](https://github.com/EffortlessMetrics/ripr-swarm/blob/6902a132eb0366a31bdd1b52a30b189847c1f82f/docs/handoffs/2026-10-15-windows-subset-admission.md),
  not a fresh settings audit here. Unreadable strictness remains unknown. No
  protection changes, release, deployment or source-repository sync is implied.

## Rollback checklist — same owners

- [ ] Withdraw the candidate's readiness on a material finding and inspect
  native run/check custody before any further request. Preserve failed/cancelled
  attempts; no synthetic green fallback, Ready retoggle or slowness cancellation.
- [ ] #4987 owner restores the matching caller/callee/policy/instruction source
  generation as one coherent rollback. Coordinate with #4986/#4836 so existing
  required enforcement remains valid. Never restore only the old empty-file
  guard beneath the new aggregate. Pending-request replacement and post-final-read
  freshness/cancellation races remain native limitations.
- [ ] Docs owner reverts the bounded integration and its fragments if necessary;
  retain original source commits and #8008 custody. #8024 owner reverts producer
  and consumer together if needed; retain receipts under their original subjects
  and mark unsupported reuse unavailable. A rollback is not permission to trust
  an unbound pass receipt or weaken validation.
- [ ] Integration owner verifies the rollback tree, required result and remaining
  holds. Preserve unrelated branches and durable evidence. No live setting,
  credential, security or billing change belongs to this checklist.

## Fresh handoff verification and limits

Linux/Bash, absolute installed rustc 1.95.0. Direct `rustc --edition=2024 --test`
on exact `6902a132` ran `windows_subset_evidence.rs`: **6 passed**;
`windows_subset_admission_contract.rs`: **7 passed**. The same evidence-test
source against predecessor `4eebbc6e` executed **2 pass / 4 intended failures**
(empty/component/malformed evidence and hidden status-write failure). Cargo
commands were mocked by these tests. Windows-only test excluded by cfg; no pwsh,
native Windows product execution or hosted run is claimed.

Independent committed-object audit of docs parent versus `5d6d6c96` checked
264 unique nonempty spec-index rows: six mismatches become zero, including the
two historical qualifiers; eight required heading omissions become zero.
This structural replay complements the linked independent semantic/native docs
review; it does not replace it. #8024's reported 45 tests remain owner evidence.
No new product repair was warranted. This document changes no behavior or
documentation-system contract; no changelog fragment is needed under
[Changelog Policy](../CHANGELOG_POLICY.md).

Handoff hygiene attempt: `cargo --offline xtask worktree doctor` exited 101
before execution (locked tokio-util 0.7.20, cached 0.7.19). A separate locked
online retry under a 90-second bound exited 124 during dependency/product
compilation; doctor itself never ran. No doctor, check-fast or precommit pass
is claimed for this document. Direct focused tests and the docs audit above
completed independently. Whitespace and the handoff's local-link target pass.

GitHub CLI reads were denied: `POST https://api.github.com/graphql` (PR search)
and `GET /repos/EffortlessMetrics/ripr-swarm/commits/6902a132eb0366a31bdd1b52a30b189847c1f82f/pulls`
both returned `Forbidden`. Neither denied call was retried. Supported GitHub app
issue/PR readers supplied the linked records. PR reads for 8009/8020 returned 404;
their issue reads succeeded. No cause or protection state is inferred.
