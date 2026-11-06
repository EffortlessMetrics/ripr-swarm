# Isolated LSP full-refresh cancellation handoff

Outcome: one bounded scheduler repair is ready for central source/integration review.
No merge, release, central-branch write, or integrated qualification occurred.
Observed/reviewed: 2026-11-06T03:37:28.186213+00:00.

## Exact identities

- Repository: EffortlessMetrics/ripr-swarm.
- Assigned base: `b51aebc7e4735529befd677ebf14071c7f09e477`, tree `d3142dd59d4dcfea5e6b4981732f61c8516f3fa0`.
- Final source: `5416f4ef0607585fa9216d43c22104bc923866b6`, tree `5ce610f2ba5ae58c028e8b9faed1686321912fd3`.
- Source branch: `worker/lsp-cancel-full-5416f4ef0`.
- Runtime-tested source: `a9c4d36dcf650bca80745ea009008791dec7fa18`, tree `12a88ba6e081e0031ba125cb93c3dd5087711173`.
- Final source differs from the tested source only by correcting an ordering claim in
  RIPR-SPEC-0180. `git diff --quiet a9c4d36dc HEAD -- crates .ripr changelog.d`
  returned 0. Product/test/traceability/changelog proof is carried forward with its
  original tested identity; `git diff --check` also passed on final source.
- Actual test executable SHA-256: `ea99a2f394d376144c5d71b0967581be2132e964fcdfdc1a28d3c7048a0bf13c`; 301515304 bytes.
- Cargo's compiler-artifact fingerprint confirms exactly `["lang-rust"]`.
- The scheduler in central source `763bc25700ae4bfe86141915e00479f55a99e523`
  (tree `e5d68e5d5a24b64d6af122db69d14a16f22fc70a`) is byte-identical to the assigned
  base: SHA-256 `a780e3bc75cdbc8a030cc13cccff105031f806563254591ab37bc6303e320617`.
  That is source comparison only. No combined-tree test is implied.

## Repair and proof

A newer accepted saved input increments the scheduler generation, making the old
full result ineligible to publish. The previous Full-to-Interactive exception
nevertheless kept its token live, so the newest edit waited for discarded work.
The scheduler now cooperatively cancels every superseded active attempt. Existing
same-input/scope dedup still returns before cancellation; the newest save keeps
its previous interactive scope and `seams_deferred` disclosure.

The discriminating scheduler test failed on test-only base descendant
`4cc47d1eb7c3375020f2b8b3f7003db9eb427e45` with:
`unpublishable full refresh must stop cooperatively for the newest edit`.
That was a behavioral failure (one selected test), distinct from the earlier
missing-import compile failure retained in local evidence.

The persistent test uses real initialize/open/change/save/pull/shutdown handlers,
the existing analysis worker/execution gate, and the existing producer barrier.
It edits production from `!flag` through `flag && false` to `flag || true`, changes
related assertions, and checks version 3 of production and version 2 of the test.
After three queued saves there are two attempted jobs, one superseded attempt,
one published newest snapshot, three nonempty findings, and one unchanged-file
fact-cache hit. The active token is superseded and the worker observes its abort.
Probe/class/RIPR/related-test signatures equal an independent same-state analysis;
saved-byte digests and unchanged-save/result-id reuse are also checked.

The fixture first completes a valid full snapshot including the unchanged
companion. An earlier fixture incorrectly expected cache hits without priming that
file: the old discarded full refresh had incidentally warmed it. Its failed
candidate log is retained; it is not reported as a product regression or a pass.

## Validation

Environment: Linux 6.18.57 x86_64, four-CPU quota, 16 GiB cgroup, no swap;
`rustc 1.95.0 (59807616e 2026-04-14)`.

Build command (same test settings for red and green):

```sh
RUSTUP_HOME=/workspace/.rustup CARGO_HOME=/workspace/.cargo \
PATH=/workspace/.cargo/bin:$PATH CARGO_BUILD_JOBS=1 \
CARGO_INCREMENTAL=0 CARGO_PROFILE_TEST_DEBUG=0 \
cargo test -p ripr --no-default-features --features lang-rust --lib lsp:: --no-run
```

All commands ran behind a task-local guard: 1,200 seconds for the complete tree,
10 GiB aggregate sampled RSS, 14 GiB cgroup usage, 10 GiB disk reserve. No cap was
raised. Guard receipts include native exit, wall/CPU/RSS and owned descendants.

Each following selector ran against the compiler-selected executable with
`--nocapture --test-threads 1`; nonzero selected counts were required:

| Group | Tests | Result |
| --- | ---: | --- |
| scheduler | 18 | pass |
| persistent | 1 | pass |
| saved-sequence | 21 | pass |
| consumed-source | 17 | pass |
| analysis-thread | 1 | pass |
| progress | 3 | pass |
| shutdown | 3 | pass |
| framed-edit | 1 | pass |
| deadline-cancel | 4 | pass |
| debounce | 1 | pass |
| exact unanswered-create liveness control | 1 | pass |

Total: **71 distinct focused tests passed**. The ten primary selectors are in
`raw/final-matrix.json`; the extra exact selector is
`lsp::tests::progress_create_liveness_tests::unanswered_create_does_not_delay_real_refresh_or_shutdown`.
The latter passed on this Linux candidate, including a real failed refresh and
shutdown with the progress-create response withheld. The separate reported
native-Windows failure was not reproduced or cleared here.

`cargo fmt --all -- --check`, `cargo xtask worktree doctor`, and native
`check-traceability`, `check-process-policy`, `check-no-panic-family`, and
`check-allow-attributes` passed on tested source. Doctor's retained-target-report
warning is expected while these witnesses are retained. Building xtask also
compiled the default-feature product library; that is not default-feature test
qualification. Its absolute selected driver and hash are in `raw/policy-artifact.json`.

## Limits and remaining work

- No representative latency distribution, large-corpus result, speedup percentage,
  release envelope, or server-RSS reduction is claimed. The gate-held synthetic
  fixture proves cancellation/currentness, not tail latency. Its native test wall
  time was about 1.00 s, CPU 0.224 s,
  and largest waited process RSS 108404 KiB;
  guard wall includes sampling/wrapper overhead. This is a Rust-only libtest
  process, not the earlier large-corpus server process.
- The existing saved-edit harness passed on red and green source as preservation
  controls. It calls analysis directly. It clones the small fixture receipt for
  its self-dogfood row (`saved_edit_sequence.rs`, fixture harness), and
  `latest_save_to_snapshot_ms` begins inside `RefreshScheduler::request`, after
  the backend's 200 ms debounce. Those measurements do not establish real-repo
  or end-to-end edit-loop latency. This lane did not change the separately owned
  scoreboard/measurement work.
- The first default-feature library-test build on exact base stopped at the
  admitted 10 GiB RSS cap before any tests executed (exit 124 from the guard).
  No default-feature test/full-library/all-targets/Clippy/hosted/release pass is claimed.
- `check-changelog-fragments` returned 1 for
  `changelog.d/12010-sink-identity.md`: its bullet has no issue/PR reference. The
  exact same failure was reproduced from the complete changelog-checker input
  set extracted from immutable base b51, using the same unchanged checker.
  The replay is scoped to those policy inputs, not a base product build.
  This LSP fragment was not flagged. Central should resolve the correct reference.
- `check-file-policy` started an unrelated combined build of five test-valued
  coverage targets. This lane stopped that owned operation across its distinct
  xtask/Cargo/compiler groups; status 241 is a known scope stop, not a pass or an
  authorization denial. Other independent policy checks completed. No live
  recorded owned descendants remained; defunct compiler/Cargo entries await init
  reaping. See `raw/file-policy-scope-stop.json`.
- `check-fast`/`precommit` are not claimed: the installed selector fixes
  `origin/main`, whose merge-base here is
  `551456df5b70c4a3ae9ad95e812e193d40fb5999`, not assigned base b51. The six-path
  assigned-base diff was independently resolved; no remote-tracking ref was
  rewritten to manufacture a selector pass. Aggregate native policy is also not
  green for the reasons above.
- All-state PR search via `gh pr list --repo EffortlessMetrics/ripr-swarm --state all
  --search 'LSP debounce cancellation saved edit' --limit 30 --json
  number,title,state,headRefName` was denied by the service with GraphQL
  `Forbidden`. Relevant later message evidence did not resolve access; no retry
  or alternate route was used. Equivalent open-PR coverage remains unverified.

## Integration, ownership, and resources

The designated central owner remains the sole release/integration writer. Fetch the final source branch and inspect the diff from b51;
reconcile current source before applying it. No writes touched protected public PRs, central branches, diagnostic/lens repair
#13374, document-budget #12979, type-bound reuse #12976, TypeScript/fact loading,
the captured-log scan, or qualified-Lamp scans.

Source patch is receiver-readable in the final source branch. This evidence
branch is a separate review artifact, not a requested release gate. Runtime
proof belongs to a9c4d36dc; the final documentation-only source change preserves
its unaffected dimensions. No release qualification is transferred.

All local executable witnesses are retained (older ones compressed with verified
restoration) for source/evidence review, not supplied as install/release binaries.
Task-local raw evidence and guards are retained for reproduction. The mixed
pre-existing Cargo target is retained for owner verification/warm reuse, not
purged. The exact-base changelog input snapshot is retained with its replay
manifest. No shared or uncertain process was killed. `raw/witness-closeout.json`
records archive hashes, byte-for-byte restore checks and verified private owner-executable restoration. Historic original timestamps/modes are not qualification evidence.
