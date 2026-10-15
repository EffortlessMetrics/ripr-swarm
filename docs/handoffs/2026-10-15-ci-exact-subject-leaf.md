# Staged exact-subject CI leaf (#4987)

This is source preparation for parent integration, not live admission or hosted
qualification. The existing `ci.yml` PR, main/master push and manual triggers
remain. No schedule existed and none is added. No shared routed-rust or rust-gates
caller/result file changes. Admission source was refreshed at
`0cf965fcdfca893e7ce9b89767ee7f88937f1822`; the printed-command companion is
`624177123a396ddb3272196ffe4d3349d1305470`. Wheel and Windows remain separate
owners. This slice was based on main `0c01cc9127de8f886b961fd003680e05035dcbcf`;
subsequent main `94a42072e4b348f7a10aaa69ba4e19b5a7106a58` changes none of its
workflow, existing routing-test or policy inputs.

## Callable contract

The trusted caller supplies required strings `proof_profile=qualification`,
`request_id=<run>-<attempt>`, `head_sha`, `base_sha`, `subject_sha`, `subject_tree`
and `workflow_generation`. H/B/subject/tree/generation must be full lowercase
40-hex identities. The source job verifies the checkout, tree, ordered B/H merge
parents and `GITHUB_WORKFLOW_SHA`. This binds bytes; it does not authenticate review
or perform admission. The caller remains the sole freshness/authorization owner.

Four required booleans are frozen selection intent for that request:

| Input | Complete retained payload |
| --- | --- |
| `run_rust_only` | Locked fixture dependency preparation, then `cargo test -p ripr --locked --no-default-features --features lang-rust --no-fail-fast` |
| `run_release_proof` | Perl feature check and existing filtered Perl library runtime, package list, publish dry-run, unlocked packaged dependency re-resolution/check and release readiness with receipts |
| `run_msrv` | Rust 1.95.0 `cargo check --workspace --all-targets` |
| `run_vscode` | Node 24 locked npm install, compile, package, xvfb and Electron e2e, with failure-only host diagnostics |

At least one row must be selected. Each selected child depends on successful
identity validation and checks out `subject_sha` with credentials persistence
disabled. Selected Rust runtime bypasses legacy PR diff pruning; unselected
rows must be skipped. `ci-result` requires identity success, every selected child
success, every unselected child skipped, and rejects cancellation. The caller
must require the reusable job's native `needs.<leaf>.result == 'success'` whenever
selected. A missing/skipped/failed/cancelled leaf is not success. There is no
candidate-authored success artifact or output that can replace the native result.

Permissions stay `contents: read`, without secrets or write escalation. The
concurrency key includes request and subject; PR synchronize cancellation applies
only to legacy runs. A parent must use a distinct job/workflow concurrency key
and retain its own revocation/cancellation logic. Every callable payload job is
capped at 20 minutes; identity/result jobs at two minutes each. This is a per-job
execution cap, **not** measured <=20-minute end-to-end qualification, queue-time
or cancellation-delivery proof. Parent admission must budget/enforce the complete
portfolio deadline; do not promote these declarations to that acceptance.

The original command payloads, actions, feature flags and test filters are pinned
against `fixtures/convergence/ci-exact-subject/original.yml`. Two explicit changes
are permitted: selected callable Rust bypasses event-diff pruning, and unlocked
install uses sequential checked commands plus one checked package-version capture.
The latter repairs reproduced masking of failed package, pkgid and cd commands
without changing install/package coverage. Missing dependency history, invalid or
ambiguous merge bases, empty/failed diff and awkward/deleted/renamed paths retain
the existing conservative Rust classification through permanent real-Git tests.

## Explicit activation delta — parent-owned, not applied

1. Integrate this leaf together with the admitted caller and fail-closed required
   aggregate under #4987/#4836. Keep review early and substantive on pushes; no
   heavy proof is implied by Draft/open/synchronize/Ready or ordinary labels.
2. At the fresh explicit `merge-ready` request, snapshot `full-ci` and
   `release-check` as selection intent. `full-ci` selects retained full rows;
   `release-check` selects the release row. Use the canonical product-gate plan
   for affected rows and conservative unknown handling; no second YAML path
   classifier belongs here. Unknown/incomplete applicability must widen or reject,
   never silently turn a required row false. Stored labels are not future retry
   authority. A new subject or failed attempt needs the caller's fresh admission.
3. Pass the immutable admitted H/B/subject/tree/generation and all four explicit
   booleans. The caller must not omit a selected leaf, credit a skipped child, or
   use a stale predecessor's success. Keep existing admission-owner duplicate,
   review revocation and subject replacement behavior.
4. In the **same combined integration**, remove exactly `ci.yml`'s `pull_request`
   block and main/master `push` block. Retain `workflow_call` and the existing
   `workflow_dispatch`. This yields zero automatic leaf allocation on Draft,
   open/push/Ready/unrelated labels. The permanent activation matrix exercises
   that exact source delta. Do not add native merge queue or main-push replay.
5. Qualify the composed caller/leaf/result on hosted runners before claiming
   enforcement or economics. Confirm selected commands actually executed for the
   immutable subject; preserve failed/missing-child rejection and measure the
   complete <=20-minute target separately.

Manual semantics remain: Perl checks/runtime plus Rust-only, MSRV and VS Code;
packaging/readiness remain excluded on a bare manual run as before. No scheduled
CI semantics are introduced. Other workflow schedules are outside this slice.
Revert the callable addition and its caller wiring together for rollback; the
legacy trigger retirement must never land before its replacement is enforceable.

## Owner coordination and evidence limits

#7159 owns expanding affected Perl runtime beyond the existing substring-filtered
library denominator and decoupling it from release packaging. This patch does not
claim that broader runtime coverage: `run_release_proof` retains the existing row
as one unit. Do not credit all-feature compile or default tests as Perl runtime.
#4836 retains actual-subject producer and protected aggregate ownership; no
PR-evidence command, feature population or shared caller is changed here.

The dedicated Linux source contract executes literal Bash guards/predicates with
bounded stubs, checks wiring/removal controls, compares complete payload blocks,
and reproduces native command failures. It does not run Rust product tests,
Perl runtime, package/readiness builds, VS Code/Electron or native admission APIs.
The existing ten Rust routing tests exercise real private Git DAGs and unknown
input refusal/widening. Actionlint checks Actions syntax/context availability;
source policy checks and local contracts remain separate from hosted proof.

Changelog: `changelog.d/4987-ci-exact-subject-leaf.md`, canonical Internal / CI
fragment with issue reference. Local worktree `/workspace/ci-4987` and proof
scratch `/workspace/ci-4987-proof` remain retained for parent continuation. The
10-GiB additional build budget and 15-GiB reserve apply; no existing resources
are deleted or relabeled as delivered hosted evidence.
