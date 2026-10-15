# #8004 operand-item guard evidence

Compact technical evidence for repaired candidate `def018d1a67109fa327f094cd63c38b936a407e0` (tree `a8c022a078c40285c8e38624c5c0431f9e907531`, parent `f6aab7639b88be1a21c757be74baef833e636f17`). The published head is metadata-only commit `15ca898ad1d3571d104ce1ae20b629fe2f55fc86`, independently read back with the identical tree/sole parent and empty source diff. Current PR readback: [#8004](https://github.com/EffortlessMetrics/ripr-swarm/pull/8004) is open, non-draft, and unmerged. Ready-to-start-checks prerequisites and transition are verified. Required hosted checks and merge clearance remain pending. `public-handoff-draft.md` is provided for review.

## What the evidence establishes

- Candidate source manifest: all 16 listed source files matched their SHA-256 values in the retained clean checkout. The exact-head default-feature focused unit run passed 2 tests with 10,814 filtered; the six-test receiver integration target passed; author `check-fast` passed 15/15 selected gates, with no skipped gates, against frozen `origin/main` `0e8d006e9845339a58902bb2baa28dbc4fd23c18`. Workspace all-target Clippy with `-D warnings` passed in that selected gate set. The author receipt does not retain the integration-test argv or check-fast wrapper argv; the test target, test output, exact child `fmt`/Clippy commands and the repository-prescribed `cargo xtask check-fast` command are recorded transparently. See `author/`.
- The clean author unit-test binary identity is recorded with its `def018d1`/`DIRTY=false` build stamp. The independent candidate-control CLI remains separately identified as a precommit source-equivalent `f6aab763…-dirty` executable. It is not promoted to a final-head or release binary.
- Independent control evidence covers 10 fixture sources × 3 report surfaces. All 30 source digests match their fixtures; the old control reports the unit-struct operand as `direct_owner_call`. The Rust 1.95.0 native matrix passes 10/10; a mutation of the private Object branch yields 9/10 with only the genuine Object positive failing. No binaries are included. Compact report completeness is not overstated: its schema has no typed `run_status`. See `review/`.
- Frozen local current-main composition `b960feed712cbb71e6d07c9f902e057a677261c3` (tree `159f28028a3e91cd4256fa2b95d0c65c2d5c5354`, parents `2946fc9be09dd1e4f2a248c209561f718aa976df` + `def018d1a67109fa327f094cd63c38b936a407e0`) passed four bounded commands, all exit 0: two relation units (10,844 filtered); six receiver integrations (0 filtered); 17 targeted `cli_smoke` rerun tests (502 filtered); and five `native_path_roots` tests (0 filtered). All four exact argument vectors, subject identities, terminal counts and full-log hashes are in `composition/qualification-summary.json`; selected test output is in `composition/*-excerpt.txt`. A fresh clean b960 CLI separately passed all 30 independent fixture reports.

## Open boundaries and exact published state

GitHub readback confirms the PR is open, non-draft, unmerged, with head `15ca898ad1d3571d104ce1ae20b629fe2f55fc86`. Do not infer clearance or passed hosted checks from this workflow state. The reported GitHub merge/test object is `2fa3df5b4eca7cfb8376a676b2380a59aaf02c35`, tree `d1c80eaa9192ca99b11011221bc80d0b342f9010`, parents `9543a9bd733b0921886306abd2c2609fbe73b62e` + `15ca898`. The same PR metadata read reports base SHA `0e8d006e9845339a58902bb2baa28dbc4fd23c18`, which differs from the fetched merge/test object’s main parent. Both are preserved; the actual hosted job checkout remains unverified.

The older immutable packet remains the source of record for known optional hosted gaps and is linked rather than duplicated: [evidence/8004/followup at commit `7dafbebe35d5e3c2cd0b66331969cb300623d463`](https://github.com/EffortlessMetrics/ripr-swarm/tree/7dafbebe35d5e3c2cd0b66331969cb300623d463/evidence/8004/followup). For the recorded earlier head, default cargo-test failed its canonical fixture-dependency test (#7920), and an analyzer lane stopped before analysis because 1,229 Rust files exceeded its 800-file cap (#7903/#7915). These gaps remain open for their recorded evidence; this repair does not convert them to passes. The nonblocking receiver-parse performance follow-up remains open under [#8005](https://github.com/EffortlessMetrics/ripr-swarm/issues/8005); broader #7415 limitations remain open.

Ready-to-start-checks prerequisites and transition are verified. Required hosted checks and merge clearance remain pending. This packet does not claim full default-suite, hosted job checkout/qualification, release-binary acceptance, or UB-review adoption clearance. No all-threads-resolved claim is made.

## Contents and integrity

- `identity/author-source-manifest.json`, `identity/source-verification.json`: candidate identities and verified source SHA values.
- `identity/current-pr-readback.json`, `identity/published-merge-object-observation.json`: current PR state and reported merge/test object identities, with checkout limits.
- `author/`: focused unit/check-fast outputs, intended old-guard red, first overstrict test-oracle failure and correction, fixture/corpus/policy results.
- `review/`: independent review summary, 30-report control matrix with raw-report hashes, native sources and selected raw logs.
- `composition/`: exact commands, local subject identities, terminal counts, selected output excerpts and full-log digests.
- `identity/omitted-artifacts.json`: SHA-256 and byte count for omitted full reports, large logs and executables.
- `identity/bundle-file-manifest.json` and `SHA256SUMS.txt`: packet integrity.

Compiled executables and raw full-control reports are hashed but omitted. Full verbose Cargo JSON logs are hashed and represented by selected terminal output excerpts. No credential or private transfer reference is included.
