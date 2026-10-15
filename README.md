# #8135 prerequisite-only source refresh

Bounded parent-authorized normal merge/push completed. PR8135 remains open/Draft at new head9b0945b202cbb99ca0e708c21ba9879b7aaf71bd. No tracked content differs from the previously qualified tree. Independent exact-new-head review found no blocking source/integration/local-proof issue; the stale hosted-subject prerequisite gap is closed for the observed966e object. Required hosted qualification/job checkout remains parent-owned and unestablished.

| Identity | Exact object |
| --- | --- |
| Original reviewed implementation head | 7d246801183e22bcf78e6c2efd556e387e27b1e2 |
| Frozen admitted main | 758a7c082c1c140d1d818bb3d7e5be78643d3f22 |
| New submitted source | 9b0945b202cbb99ca0e708c21ba9879b7aaf71bd |
| New source parents, ordered | original implementation head + frozen main |
| Qualified source/merge tree | 48bf287c742186117177822ae098a8bbe26fbfa9 |
| Prior executed composition, identical tree | 05e332b66af067a1a365f4d1f5af18e32fd925f2 |
| Actual refreshed GitHub merge | 966e5aa88559d1793eb721c39f1431f81ffdd798 |
| Actual remote merge parents, ordered | frozen main + new submitted source |
| Material prerequisite #8102 | 98c8f172bfd6aa2b808f63431ed4eac9c224c102 |
| Executed/remote prerequisite fixture blob | 3ebc6faf435a443c6baa31bf410e53d121cadf6f |

Fresh new-head proof (all native exit0):
- `cargo xtask check-fast`:13 gates including strict workspace/all-target Clippy; stable4-path selector against frozen main independently checked.2 inapplicable skips retained (support-tier/fixture-contract). No empty selector claim.
- `cargo xtask precommit`: local_precommit at exact9b head/base758a, same4 changes; selected ripr/xtask Clippy passed, no recorded skips/limitations. Independently recomputed report digest sha256:6d56a16939181f3043fe9924d7a0581e8f3cc45881aba34fb95beea9bb0115f0.
- `cargo test --locked -p ripr --no-default-features --features lang-rust --test cli_smoke structural_workspace_markers`:3 passed,0 failed/ignored. Feature-reduced warnings remain in raw log.
- Same Rust-only CLI target, filter `version_is_exact_and_precedes_help_or_output_flags`:1 passed,0 failed/ignored. This existing independent Git-derived oracle checks head-sensitive build metadata.
- Actual `/workspace/8135-refresh-target/debug/ripr --version`: exit0, empty stderr, exactly `ripr 0.11.0 (9b0945b202cbb99ca0e708c21ba9879b7aaf71bd)` plus newline. Binary sha256:6e3c2eb8cb967e11d5dc581fbd32c155e80d8e9d1514ec6b66437c1694ddc1f7.

Evidence reuse is explicitly source-level: previous116 ledger /5 selector /31 diagnostics remain original05e332 executions, not fresh9b tests. Exact tree/input/oracle equality plus independent review supports unchanged-source conclusions. build.rs changes emitted HEAD/cache identity despite equal source trees; fresh policies, runtime control and version test cover the changed dimensions. See evidence-reuse-map.md and independent-new-head-review.md.

Previous raw packet: https://github.com/EffortlessMetrics/ripr-swarm/tree/e1d5e75fa721b0daa4fc404153b126628da0463a (archive sha256:bdf7114c4cba33fcb498ee97fda08adec5012a6b5e42fd69fe5569d910ab1361;92 hashes previously restored/verified). Its zero-subject diagnostic attempt remains non-evidence. Historical hosted job116522145229 on a8bc/old source remains FAILED; neither exact-tree reuse nor the new source push turns it green. Original repaired-P1 comparison packet3b0c45ed remains linked there.

Fresh ownership/head checks matched the parent-authorized exclusive branch at7d, connected PR author/actor identity and no intervening branch work. Before push, clean new source/tree checked again; remote still7d. Normal non-force push7d->9b exited0. Both connected PR read and native refs confirm new source9b, Draft, base758a,4 changed files. Independently fetched/inspected actual remote966e: parents758a+9b, tree48bf, #8102 ancestor and fixture3ebc; tree diff to new source exit0. A pre-Ready merge-ref observation does not establish any actual hosted-job checkout.

Current retained snapshot contains all3 threads,6 reviews,22 check runs. No new substantive finding; original P1 outdated/repaired, optional namesake precision and panic cleanup nonblocking. Threads remain unresolved and unchanged for parent. New-head Rust-only job116602622669/run38787336432 was in progress; no required Ripr Rust Small Result admission/result yet on Draft. No hosted aggregate success or ongoing monitoring claimed. Parent owns current-state refresh, Ready admission, actual job checkout/terminal required clearance, findings/thread resolution and protected merge.

#8142 remains the untouched precision alternative at33be7c306c147e3b862451891164d73b2faaea91. #8150's stronger successful uv root/probe proof is not incorporated or duplicated; this tree retains #8102's Rust-only refusal characterization. No Ready/main merge, thread resolution, bot request, release/deployment, credential/security/settings change or destructive cleanup occurred.

The invalid literal-SHA remote bundle attempt produced no bundle and is retained in retained-bundle-attempt.json; its native status was not separately captured, so later aggregate success is not assigned to it. A named task-owned local evidence ref then produced a verified remote-subject bundle with exit0. This packaging correction changed no source or test.

Run `python3 verify-receipts.py` for independent digest/count/selector/identity/push/remote-subject checks, and `sha256sum -c MANIFEST.sha256` for bytes. source-refresh.bundle advertises9b and requires old source7d + frozen main758a; remote-merge-subject.bundle advertises966e and requires9b +758a. Both verified in the source repository. Original/raw logs, exact commands/exits, source/remote objects, environment/profile, selector/report proofs, independent review and resource receipts are included. Original executable bytes remain local with original hashes; compact packet records new binary identity without claiming release-artifact acceptance.
