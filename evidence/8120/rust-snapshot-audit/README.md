# Rust snapshot audit, 2026-10-16

**Verified current finding:** repository seam grading can claim `strongly_gripped` / `discriminate=yes` for an Insta snapshot of another receiver's same-named field, or when its expected file is absent. The resulting canonical recommendation says: `No new RIPR action; current static evidence already observes this seam.` The changed-field diff findings remain `weakly_exposed`; this is not a claim that those diff findings were `exposed`.

This is a review and reproduction packet for existing issues [#8120](https://github.com/EffortlessMetrics/ripr-swarm/issues/8120) and [#8121](https://github.com/EffortlessMetrics/ripr-swarm/issues/8121), under [#6642](https://github.com/EffortlessMetrics/ripr-swarm/issues/6642). It does not create a competing implementation, new issue queue, source PR, release qualification, or release prerequisite. Shared observer work remains with #3161/#4793; existing inherent-receiver PR #8004 addresses a separate claim.

## Exact subjects and execution

- Repository: `EffortlessMetrics/ripr-swarm`.
- Reviewed and built main: `e00018da7d224bd13b4c4e3466d1e626ca0b15f6`.
- Executed CLI: SHA-256 `cac488c0fbd606c87f071b9b8f6c838b68cb19f06ab289823fd4ed70e9080d53`; its Cargo compiler-artifact message selects the absolute executable consumed by the audit. `--version` prints the full clean source SHA.
- Pinned Rust/Cargo 1.95.0, Linux x86_64, Bash, `--locked`, `--no-default-features --features lang-rust`, dev debug information disabled, incremental disabled. One Cargo job; one test thread. Analyzer thread environment is separately bounded; it is not a claim about arbitrary CLI child-process pools.
- Exercised fixture: the retained `authored-lib-insta` source/manifest/lockfile/snapshots, verified against its committed retained-file SHA-256 inventory before materialization. Actual library versions: Insta 1.49.0 and serde 1.0.229. `CI=true INSTA_UPDATE=no` for every test.
- Earlier `d08c77d144c016078364fcc1fcf2fa593d21cefc` build and 14-variant receipts are retained as predecessor evidence only. The 16-variant matrix below was rerun using the separate exact-main executable. No source/build/install equivalence is inferred from version strings or matching source files.

## Observed controls

Each matrix variant executes all three fixture unit tests, with zero ignored tests. The 16 variants therefore represent 48 fixture test executions, rather than 48 distinct authored tests. Eight Cargo commands pass and eight deliberately fail at a snapshot assertion. All 16 test stderr logs explicitly show compilation of their intended fixture path. Each public analyzer command exits zero with a nonempty finding; the selected repository reports have `run_status=complete`. Zero-case doctest summaries are excluded from the execution denominator.

| Control | Actual unit tests | Relevant public result |
| --- | --- | --- |
| Direct owner `record.items`, unchanged owner | 3 pass | field strongly gripped / discrimination yes |
| Same direct snapshot; owner returns `items.len() + 1` | 2 pass, 1 fails | field strongly gripped; diff weakly exposed |
| Wrong owner value; snapshot is `other.items` from an unrelated record containing literal 2 | 3 pass | **field strongly gripped / discrimination yes**; diff weakly exposed |
| Same wrong receiver fixture; remove only the snapshot assertion | 3 pass | field weakly gripped / discrimination weak |
| Same direct snapshot; expected file missing | 2 pass, 1 fails from missing expectation | **field strongly gripped / discrimination yes** |
| Redacted `id` changed | 3 pass | id field weakly gripped; diff weakly exposed |
| Unredacted `items` changed under the whole-record redacted snapshot | 2 pass, 1 fails | items field weakly gripped; diff weakly exposed |
| Only sibling `record.items` snapshotted while `id` changes | 3 pass | id field weakly gripped |
| Production `items` and expected snapshot both updated | 3 pass | diff weakly exposed; passing comparison is not independent correctness evidence |
| Boundary owner changed, snapshot instead observes `vec![1_u64, 2, 3]` | 3 pass | diff infection unknown / discrimination weak |
| Original boundary snapshot and deliberately wrong boundary value | 2 pass, 1 fails | diff infection unknown / discrimination weak |
| Expected boundary snapshot stale, missing, wrong test filename, or `.snap.new` only | each 2 pass, 1 fails | identical limited diff classification; these setup failures are not behavioral mutation detection |

The full 16 rows, exact test names/outcomes, source/diff hashes, and output projections are in `summary.json`. Public paths exercised are `ripr check --root FIXTURE --diff FIXTURE/change.diff --format json` and `ripr check --root FIXTURE --format repo-exposure-json`.

An additional expectation-only sequence leaves the Rust source bytes unchanged (SHA-256 `30f51c2895cfd983de17e849c5bf103dce0b2c64205def65c46369c1a0f144c8`), then uses expected value 2, stale value 99, no expected file, and restored value 2. Native results are respectively 3 pass; 2 pass/1 fail; 2 pass/1 fail; 3 pass. All four repository projections remain strongly gripped/discrimination yes with the same no-action recommendation. A separately materialized cold missing-file report also has that result. This establishes an expectation-reader/observation limitation, not a cache-only diagnosis. These four commands add 12 executions of the existing three tests, separately counted from the matrix.

## Cause, relevance, and repair handback

At the reviewed SHA, `analysis/test_grip_evidence.rs::field_construction_oracle_matches_seam_field` (line 2295) accepts any member/record field spelling after stripping comments and strings. It does not establish that `other.items` is the returned `record.items`. `discriminate_evidence` (line 2024) promotes a matching medium oracle to `StageState::Yes`. `analysis/repair_route.rs::apply_grip_class_ceiling` (line 177) then maps strongly gripped to AlreadyGripped and clears missing evidence; `output/evidence_record.rs` line 891 renders the no-action recommendation. The common assertion classifier still grades Insta as snapshot/medium; no committed Rust `.snap` binding was found on these paths.

Snapshot framework/name/file presence does not establish observation of the changed source field. Reuse #8120's exact assertion/expected-bytes/settings admission, followed by #8121's field/receiver/projection relationship. An unsupported or absent relationship must withhold full discrimination. Keep the actual-owner positive, wrong-receiver negative, assertion-removal negative, redacted-id/sibling distinction, missing/stale expectations, and production-plus-expectation update controls. Graduate confirmed false promotions through the existing honesty corpus when implementing; do not globally promote or discard every snapshot.

Confidence is high for the reproduced repository grading and recommendation. No product repair is submitted by this packet. The existing issue owners own implementation, exact candidate review, appropriate local gates and any later hosted qualification. Nothing here clears a source PR for Ready/merge.

## Instrument correction and limits

An initial repeated expectation-only attempt reused another same-name fixture's executable from the shared Cargo target. Its stale-case expectation failed to fail, and its unused-variable warning revealed the wrong compiled fixture. That attempt is excluded and retained under `evidence-current/snapshot-only-invalid-attempt`. The corrected sequence uses an exclusive `snapshot-only-target`, compiles the intended baseline and then varies only runtime snapshot files. All 16 accepted matrix runs independently have their intended compilation-path lines. The replay driver now refuses a matrix result without that line. No failed or zero-subject setup is reclassified as a successful behavioral control.

These are Rust-only local audit results. The feature build emits inherited warnings; no strict Clippy, full default-feature workspace suite, native Windows, hosted CI, package/install rehearsal, support promotion or release qualification is claimed. No hosted run was requested, and the deliberately failing fixtures are not submitted as a green source candidate. Runtime comparison and static exposure remain separate evidence.

## Recover and replay

`audit-evidence.tar.gz.b64.txt` is a Base64-encoded gzip tar containing the exact fixture inputs, raw stdout/stderr/command receipts, compiler-artifact messages, toolchain observations, and replay scripts. It contains no executable, Cargo registry, credentials, or build target. `archive-manifest.json` gives every member's size and SHA-256; `SHA256SUMS` covers the other published files. Decode into a new empty destination and verify the complete member inventory before use:

```sh
base64 -d audit-evidence.tar.gz.b64.txt > audit-evidence.tar.gz
tar -tzf audit-evidence.tar.gz
mkdir restored
tar -xzf audit-evidence.tar.gz -C restored
```

For a fresh Linux/Bash replay, select the exact source commit in a review-owned checkout. Use a new empty task root and the pinned toolchain; retain the actual emitted binary and its compiler-artifact receipt rather than assuming an installed binary is equivalent:

```sh
export SNAPSHOT_AUDIT_ROOT=/absolute/new-task-root
export SNAPSHOT_AUDIT_SOURCE=/absolute/checkout-at-e00018da
export SNAPSHOT_AUDIT_BINARY="$SNAPSHOT_AUDIT_ROOT/target/debug/ripr"
export CARGO_BUILD_JOBS=1 RUST_TEST_THREADS=1 CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0
mkdir -p "$SNAPSHOT_AUDIT_ROOT" "$SNAPSHOT_AUDIT_SOURCE/target"
cd "$SNAPSHOT_AUDIT_SOURCE"
timeout --kill-after=10s 900 cargo build --locked -p ripr --bin ripr --no-default-features --features lang-rust --target-dir "$SNAPSHOT_AUDIT_ROOT/target" --message-format=json-render-diagnostics
AUDIT_FIXTURE_DIR=fixtures-current AUDIT_EVIDENCE_DIR=evidence-current python3 /absolute/restored/audit.py
python3 /absolute/restored/snapshot_only.py
```

The replay uses GNU `timeout` and Python as task-local audit drivers; it does not install a new product or repository workflow. Cargo fixtures use the retained lockfile, one build job and one test thread. Every individual product/test invocation has a bounded deadline. Test assertions deliberately fail in the negative variants; inspect the retained native status and intended failing test instead of expecting every command to be green.

## Resources and continuation

The local audit root is `/workspace/scratch/snapshot-audit`; the primary checkout and other workers' state are untouched. Admission measured a 16 GiB container memory limit and about 30 GiB disk free, with a 10 GiB additional-storage budget and 15 GiB disk reserve. No OOM/disconnect claim is made. The owner retains local executable witnesses and raw receipts for parent review; source Git objects and the published packet permit reconstruction. No background task, CI watcher, source writer, release or deployment is arranged.
