# Independent review of Draft PR #8201

**Current head94b92:** [independent exclusion-repair review](exclusion94/REVIEW.md). All prior blocking findings independently repaired; pre-Ready technical prerequisites satisfied. Merge remains REVIEW_INCOMPLETE, with a confirmed nonblocking refusal-location note and unresolved conversations retained. Earlier head-specific judgments remain historical.

**Latest head 490963fb:** [independent repair re-review](repair490/REVIEW.md). Earlier root-alias/lifecycle findings repaired; a new excluded-alias admission regression retains REPAIR_REQUIRED. Earlier head-specific records remain historical.

**Current-head review:** [independent review at 0ffd9456](final0ffd/REVIEW.md) supersedes prior head/integration status. **Current disposition: REPAIR_REQUIRED.** A late independent root-alias counterexample withdraws the earlier pre-Ready assessment; see the current-head findings.

**Post-landing supplement:** [#8180 integration followup](POST8180.md) supersedes the earlier unlanded-prerequisite status. Original observations below remain bound to their original subjects.

Disposition: **REVIEW_INCOMPLETE**. No actionable source or contract defect found in the inspected/executed #8169 boundary. This is an independent technical review, not merge clearance. Keep Draft. No candidate source was changed or pushed.

## Exact subjects

- Repository: EffortlessMetrics/ripr-swarm; governing issue #8169.
- Reviewed head: `6a47f5bf094e5b010845b544f9464333a8b90d7d`; tree `2a15b485aa9684ff9ea05159e429641433dd550b`.
- Author integration basis: `b772d9e13cafcdc84a19ea77d9b90fbc6cb91ecc`.
- Separately reviewed main: `fbc8f7e58f11f25a8196495dfb88e7b393a5f4c4`.
- GitHub merge subject, executed independently: `0b562e491cbb2740fccb23505641be646a5a7e83`; parents main/head above; tree `1990f84a574e410c4a6599f4cbe9e399e66289d0`. Independent `git merge-tree --write-tree` agrees.
- Candidate native binary: SHA-256 `17ce729bd5016c1e9edba48fdc514ee160af626a8f933d0ead870f8a3f899d57`; `--version` reports exact candidate SHA. Independently compiled with locked dependencies/default Rust+Python+TypeScript features, Rust 1.95.0, Linux, dev/test profile, incremental off. It differs from the author-built executable; no byte identity with that executable is claimed.

## Inspection

Read root AGENTS.md, `.agents/skills/review-pr/SKILL.md`, repository review/security/Rust/context policy, controlling issue, complete 15-file patch, PR handoff and author review 5543280625. Followed CLI parsing and effective configuration through app/check, actual diff pipeline, source/module reads, human preparation, navigation, native renderers, source stamping, suppression policy, custody inventory and manifest-last publication. Inspected help/schema/spec/traceability parity and the separate main delta. No classifier/cache or UB consumer/pin work was undertaken.

The semantic owners remain native analysis and native projections. Fan-out defaults to the existing human analysis preparation and separately supplies machine navigation without the human stub route. Badge config-selected suppression remains distinct from unsuppressed detail/human projections. Native typed outcomes own exit behavior; manifest publication alone does not imply complete analysis.

## Independently observed passes

| Subject | Command / experiment | Result |
| --- | --- | --- |
| Exact candidate | `cargo test --locked -p ripr --lib fanout_` | 14 passed, including 12 new controls and 2 incidental existing matches; 11,054 filtered out. Actual native pipeline-entry counter asserts one invocation. |
| Exact candidate | `cargo test --locked -p ripr --test cli_smoke native_fanout` | 9 passed; 532 filtered out. Rust/Python/TypeScript byte parity, nondefault config/mode, suppression/date, Python navigation/nonempty source stamps, limited JSON budget/counts, native refusals, destination/interface/external-read controls. |
| Exact main composition | Same focused CLI command | 9 passed; 532 filtered out. Separate Cargo build at actual merge subject, with unchanged fan-out/source-boundary blobs and unchanged Cargo manifests/lock relative to candidate. |
| Frozen candidate binary | Independent driver, final attempt | 50 successful experiment processes, where expected nonzero command outcomes are asserted. See `independent3/independent.json` and raw streams. |

The independent driver compares all native bytes with zero normalization, validates every manifest member length/hash and empty fan-out stdout, and exercises effective mode/config, future/expired config-selected suppressions, strong and weak Python findings/navigation/stamps, malformed config refusal, malformed diff unsupported outcome, and advisory unhandled-language scope. Source/test digest assertions bind actual fixture bytes. Existing candidate CLI controls separately exercise limited JSON and malformed budget/suppression refusal.

Persistent diff/config/source/suppression/directory-membership mutations were triggered on the real CLI's `analyzing` progress event, after custody binding. Each exited 2 with the drift diagnostic and did not admit a successful complete set. Destination-parent symlink retargeting outside the captured source root also exited 2 with the destination-parent diagnostic; any emitted refusal manifest remains `complete_set=false`. A reached Rust include into `.git` refused fan-out while standalone JSON remained usable. Unit controls additionally cover facts, date changes, outside-root worker reads, auxiliary binding, size caps, member/manifest collision and late drift.

A real Linux `RLIMIT_FSIZE=1024`, with SIGXFSZ ignored, caused a short badge-file write: exit 2, an actual 1,024-byte partial file, and no `manifest.json`. This exercises the real CLI writer rather than an invented helper filename.

## Important distinctions observed

- Go-only changed scope is native `partial_with_limitations`, `analysis_complete=false`, exit 0; its output set has both publication booleans true. Consumers must inspect native states.
- Malformed diff content is typed native `unsupported_input`, exit 2, `complete_set=true`, `command_success=false`.
- Invalid configuration is a pre-analysis refusal, exit 2, both booleans false, empty human bytes, native machine refusal bytes.
- A strongly exposed Python finding in the independent fixture stamps its source; its weak gap-bearing counterpart stamps source and test. Both retain standalone byte parity.
- Input protection is **observed persistent-drift detection**, not immutable snapshot analysis. Change/revert, hostile concurrent replacement, crash durability and compiler/dependency cache integrity are not established.

## Independent timing

No builds overlapped measurement. Three alternating AB/BA pairs on 8 Rust modules, 16 native findings/probes, ready mode, shared warmed analyzer state, Rayon 2, dev binary. No cache-hit or OS-cold claim. Linux child user+system CPU and perf_counter wall include subprocess startup/capture; short samples are descriptive.

| Metric | Three separate native processes | One fan-out process |
| --- | ---: | ---: |
| Median wall | 0.093576 s | 0.034200 s |
| Median CPU | 0.097148 s | 0.035947 s |
| Native member bytes | 67,250 | 67,250 |
| Extra manifest bytes | 0 | 7,922 |

All nine timed member comparisons are byte-identical. Twelve measured processes and one excluded warmup are included in the 50-process final driver receipt. No general speedup, production UB workload or release-performance qualification follows.

## Retained failed experiments / author audit

Independent attempt 1 passed native byte parity but failed an overstrict two-stamp assertion on an exposed Python fixture. Attempt 2 reached a real partial write, then failed an incorrect expected exit 2 for Go-only scope. Native output/source inspection established the distinctions above; the corrected driver added a weak Python fixture and explicit advisory/unsupported/refusal checks. Original drivers, receipts, streams and failures are preserved; neither failed attempt is counted as a pass or source defect.

Fetched author evidence commit `c0abff3ed8c193f5b2031c5073cf5f154257a71f`, manifest SHA-256 `c5349ecba3ded9e5c566bc774f3f53757cca0bb7ac2d038609f9d8352be40bd5`; verified all 24 payload sizes/hashes. Independently recomputed recorded medians and checked paired recorded output hash/byte equality. Read final gate summaries and source challenge. These remain author-executed receipts; omitted binary, corpus/native payloads and full traceability report were not independently recovered from that archive. This review's native runs use its own compiled bytes and fixtures.

## Hosted proof / blockers / remaining owner

Observed Security run 38853841460/job 116820786244 checked out the exact merge subject above (its log records `git log -1` SHA) and failed `cargo-deny` on yanked zip 8.6.0. Base and candidate both retain the same zip 8 declaration/lock, outside this PR's 15-file diff. Canonical #8180 owns that qualification repair; inspected manifest repair `4b80f51a8616a94a5924fb28292253f054a92db4` changes xtask's zip declaration to 9. It was **not** applied to the candidate or composition. Local cached locked builds do not clear supply-chain qualification.

Connector observations listed Source of Truth/Printed Command Paste success, several skipped advisory lanes, and in-progress runs. The connector's workflow reader is first-page/PR-event-only; it is not complete hosted coverage. The later `gh api .../commits/6a47.../check-runs --paginate` request returned `Forbidden`. That read was not retried through another route. Complete check coverage/current required-gate qualification is unavailable, not all-green. Review-provider quota comments are missing review, despite a success status. No substantive inline threads were returned by the permitted thread reader.

Unrun here: full workspace/goldens, fresh registry/cargo-deny qualification, native Windows, release profile/real UB timings, required hosted merge matrix and downstream adoption. Rust-only compile/check-fast/precommit are author receipts, not independently rerun in this review. These limits retain REVIEW_INCOMPLETE. No Ready/merge/thread resolution/bot contact/release/deploy occurred. Any future source repair belongs to the candidate owner and requires new-head affected review. The denied author PR-body-link mutation was not attempted by this review.

## Resource closeout

Bounded additional storage admission: 10 GiB, minimum 10 GiB free; observed growth remained under 3 GiB, free space over 27 GiB. Review-owned source worktrees can be removed after publication; raw receipts, frozen independent binary and fixtures remain in `/workspace/8201-evidence` for replay. Inherited `/workspace/ripr-swarm/target`, cargo/toolchain state and the shared checkout remain retained and untouched by cleanup. All owned drivers terminated. Durable packet omits executable/build caches but records executable digest and reproducible source/commands; it retains final corpus bytes as JSON and native payloads/streams. Drivers are published as Markdown code blocks to distinguish review experiments from repository implementation source.
