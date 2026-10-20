## Summary

Draft/Fast package expansion walks the reverse path-dependency graph once per changed root, repeating a shared tail for every root. Replace that union with one traversal that propagates at most two distinct seed origins through each node. Preserve the exact result of the old per-root union, including seed exclusion, overlapping seeds and cycles; keep the declared-target one-hop fallback unchanged.

## Swarm / Source Boundary

- [x] Ordinary internal analyzer optimization for `EffortlessMetrics/ripr-swarm`.

## Source-of-truth Links

Governing issue or accepted claim: #8930
Acceptance delta (one sentence): Preserve reverse-scope results while limiting connected reverse-edge expansion to at most twice per edge across all changed roots.

Follow-up to landed #2970/#3616.
Spec: RIPR-SPEC-0171. Acceptance: exact expansion equivalence and at most two reverse-edge visits per connected edge regardless of changed-root count.

## Candidate Relationships

Branch: `codex/reverse-scope-union`. Base: `fe098012eeb27d10a825f3c0f8fd9f3dfc6fdddf`.
Candidate: `99ff69b63e38a3e0627d859be898e74299f8bb69`; tree: `24ae3ec11f39ed2dbec175dbb019f168e4db850a`.
No stack or replacement. All-state searches for `reverse_walk`, `path_dependencies`, `reverse_dependent_scope_expansion`, and reverse/traversal performance found no equivalent repair; #3616 owns the landed original behavior. Existing upstream candidates keep their ownership.

## Scope

One architectural seam: `analysis/workspace/path_dependencies.rs` expansion.

- Production: share the reverse traversal, borrowing graph node names. Retain the original DFS as a test-only oracle and keep its cycle-marker contract.
- Evidence: a production-path shared-tail edge-count regression, literal overlapping-seed/cycle controls, and exhaustive equivalence on all 512 directed three-node graphs × eight seed subsets, with an additional unknown seed.
- Non-goals: fallback-cache patch d821d3ba; cache config/watcher/scheduler work; changed selection, attribution, capture, disclosure, scope limits, classification, schemas, dependencies or workflows; full-repository speed guarantees; merge, release, deployment, auth or protection changes.

## Changelog / Policy / Support-tier Impact

Changelog exemption under `docs/CHANGELOG_POLICY.md`: internal refactor with no behavior, API, output or workflow change. No public timing guarantee. No policy or support-tier change, suppression, dependency or new production language.

## Proof

Linux x86_64, Rust/Cargo 1.95.0, default features, two Cargo jobs, separate target. Local baseline: doctor, check-fast (six applicable gates, zero changed paths expected), and `cargo check --locked -p ripr --all-targets` passed on the exact base. Clean `origin/main...HEAD` selector and merge base independently verified; candidate has exactly three changed paths.

The regression fails for the intended edge-count assertion on test-only `7e3995aec84b86ab52296a9a8fcdae08bcdc78c5` (native 101; 20 passed / one intended failure). Exact final candidate graph tests pass 21/21, including all 4,096 graph/seed combinations and literal controls. Existing package-selection tests pass 23/23; Rust-adapter Draft tests 3/3 and Instant path-dependency control 1/1 pass. These are 48 focused tests, not the filtered full library suite.

| Changed roots / tail nodes | Connected edges | Old edge visits | Candidate edge visits | Bound |
| --- | --- | --- | --- | --- |
| 16 / 16 | 31 | 256 | 46 | 62 |
| 32 / 32 | 63 | 1,024 | 94 | 126 |
| 64 / 64 | 127 | 4,096 | 190 | 254 |

Commands: `cargo test --locked -p ripr --lib analysis::workspace::path_dependencies::tests:: -- --nocapture`; `cargo test --locked -p ripr --lib analysis::workspace::select::tests::`; `cargo test --locked -p ripr --lib draft_diff_scope -- --nocapture`; `cargo test --locked -p ripr --lib instant_mode_does_not_expand_scope_through_path_dependencies`. Three additional guard tests passed against the retained exact-candidate executable: threshold narrowing/refusal, core-over-cap refusal before indexing, and clamping the narrow limit to the hard cap. Total focused tests: 51.

Final-head `cargo xtask check-fast` passed after the capacity closeout below: 14 gates ran, including workspace/all-target Clippy with warnings denied. Its three selected paths match the independent Git selector; CLI-help parity and fixture-contract gates were inapplicable. Native `cargo xtask precommit` also passed in232s, with all three committed paths, the exact base, impact-selected ripr/xtask all-target Clippy, and report digest `sha256:9791a115527fa294f7ea8a94b9b45f4606d24505e0fbcc0ec6be78cd1ecc8d30`. Its conditional golden step correctly skipped this non-output diff, and the separate goldens check passed below.

Initial `goldens check` exited1 for an environment gap: fixture `owner_return_pin_fallible_literal` needed locked anyhow1.0.100, absent from this Cargo home; report: zero drift entries, one fixture error. Native `goldens prepare` succeeded with the same Cargo home/toolchain. The prepared `goldens check` passed in16s: zero drift entries and zero fixture errors. No blessing or source change. Native `cargo xtask dogfood` passed in2s; this is the repository's scenario suite, not a full-repository performance scan.

RED and final GREEN executables are retained locally with copy/hash/size/mode verification; the immutable evidence archive carries their custody records, raw command receipts/logs, the guard configuration and recoverable source patch/bundle, excluding executable/cache bytes.

Infrastructure result retained: the first candidate `check-fast` ran 642 seconds and was stopped during Clippy by the unchanged 12 GiB peak-growth admission guard. This is neither a source failure nor an aggregate gate pass. The owned process group was terminal (only zombies remained); releasing this task's inactive incremental cache reclaimed 6,815,043,584 bytes before the one warm retry. The 10 GiB reserve, peak-growth cap, repository policy and product guards were preserved. No shared/original target or frozen witness was removed.

## Claim Boundary

Only the traversal edge bound and equality on the tested graphs/callers are claimed. Ordered-map lookup costs still apply; this is not an end-to-end runtime measurement or a full-workspace qualification result.

Fresh prepublication observation: main `a045441b71f23e146b7b9d32219e2b747036faaa` leaves the traversal source and spec unchanged. Later main has other changes; its combined tree with this candidate is not executed here. The qualified source/base and unaffected proof are retained without a behind-only restack. Current all-state claim searches again found only the landed original graph/scope work, with no equivalent repair.

## Rollback

Revert the coherent source/test/spec change. No persistent state or migration is introduced.

## Spec-Test-Code Traceability

RIPR-SPEC-0171 → shared-tail/count, exhaustive and literal controls → production expansion → unchanged package selection. Existing Rust-adapter tests exercise mode gates and path-dependent test inclusion. No golden reblessing intended.

## CI Economics

No workflow or trigger changes. Following substantive exact-head review and completed local proof, the qualification-only native Ready retry succeeded, and the delivery owner applied `merge-ready` once for the existing read-only routed Rust qualification. Ready status requests no CI by itself. Merge, release, deployment and protection holds remain in place.

## Continuation

Disposition: **WAITING_REQUIRED_CHECKS / REVIEW_INCOMPLETE**. The earlier inferred Draft-only restriction is replaced for qualification-only Ready and merge-ready. Native Ready retry succeeded on unchanged head `99ff69b63e38a3e0627d859be898e74299f8bb69`; merge-ready was applied once. [Required Rust qualification run39367076753](https://github.com/EffortlessMetrics/ripr-swarm/actions/runs/39367076753) was created at03:58:22 UTC. No duplicate active run existed before the request. The existing [PR-native continuation](https://github.com/EffortlessMetrics/ripr-swarm/pull/8931#issuecomment-6281003347) carries the exact candidate/basis/merge identities and owning agent's next action.

All carried-forward source review and local proof remain applicable to the unchanged head. Earlier wheel/Source of Truth passed; advisory ub-review is terminal cancelled with explicit missing sensor/proof evidence, not a qualification pass. Prior denial and terminal evidence remain preserved in [the immutable hosted archive](https://github.com/EffortlessMetrics/ripr-swarm/blob/de21bb46bbb821fca3fbd6a210636893727b04ff/manifest.json); the new qualification does not rewrite that historical record.

Current source tree: `24ae3ec11f39ed2dbec175dbb019f168e4db850a`; reviewed/tested base `fe098012eeb27d10a825f3c0f8fd9f3dfc6fdddf`. Fresh pre-request remote merge subject `7c656d3096e5658b80ccb765a831341df66d3253`, parents `d8edec61714b65774abf38ce17ba91202207de25` + head, tree `c3c01de526445a5afe087bdc2fd24b6c0d1e9296`. Newer observed main `d98f95dbca76d93624d475e451f311d427fece4c` has no material traversal prerequisite or workflow difference; native merge-tree preview is conflict-free. Actual hosted checkout must still be inspected.

The owner continues through terminal qualification or a precise blocker, preserving unaffected proof. Merge, auto-merge, release, deployment, credential, settings and protection holds remain. No final merge clearance.
