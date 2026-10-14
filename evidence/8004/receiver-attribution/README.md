# ripr-swarm #8004 receiver-attribution evidence

This compact bundle records proof for the bounded receiver-attribution change in [Draft PR #8004](https://github.com/EffortlessMetrics/ripr-swarm/pull/8004), linked to #7415. It is an evidence packet, not a merge-readiness claim.

## Object identity

The published head is `f6aab7639b88be1a21c757be74baef833e636f17`. The prior semantic review was performed on local commit `6b382125783a05e9b3353131837ed215fce70247`. The metadata bridge verifies that both commits have the same complete tracked tree (`81af6df65693db067d08e661ea0f33b3ef41342c`) and sole parent (`89ac47a603092ba7f9f55cf74999b88649621abf`), with the same nine changed paths. Thus source review and source-level results apply to the identical tree. Executable identity is commit-bound: the existing author/reviewer binaries report `6b382125…`, and no fresh CLI binary run at `f6aab763…` is established here. A later bounded unit run below did compile/test exact published SHA `f6aab763…`.

At the bridge observation, PR #8004 was open and Draft, based on `0e8d006e9845339a58902bb2baa28dbc4fd23c18`. The review base was `89ac47a…`; the newer PR base composition and hosted qualification were not established by the bridge.

## What the projections show

- The initial baseline full report has five seam rows and credits unrelated `serde_json::Value::as_object` calls as high-confidence `direct_owner_call` relations to the private `JsonValue::as_object` implementation. The initial adversarial production tests fail on the serde decoys, comment-only call, and expired binding (3 failed, 1 passed).
- An intermediate refinement made the report command exit successfully, but still allowed the wrong calls back as `helper_owner_call`. Its paired full and compact JSON reports are intentionally marked semantic red; the source filenames originally said “green” but the frozen receipt correctly records this stage as failed.
- The final default-feature basic full/compact reports are both non-empty (5 seams). Actual private Object/null calls retain direct/high attribution; wrong serde calls receive no direct/helper attribution. Public inherent and free-function controls remain. The compact public summary metrics match the full report. The nine-fixture default receipt records each fixture and full/compact digest.
- The separate reviewer Rust-only lane includes its own basic full/compact reports and isolated wrong-only / mixed-helper full/compact reports. The independent control receipt records 10 isolated controls and 30 CLI reports; valid direct positives remain, wrong-only private seams are not credited, and a genuine separate helper remains admissible in the mixed case.

## Proof lanes (kept separate)

**Author, default features:** producer SHA-256 `9707ca93255f5b2c02753ddcfafa9c0a74d7ab58db24b1bdea36ebea1a70074a`, version `ripr 0.11.0` built from reviewed local commit `6b382125…`, Rust/Cargo 1.95.0, 2 jobs, incremental off, default dev profile. Nine public full/compact fixture pairs completed with five seams each and relation/summary parity. The selected CLI rerun passed 16 tests.

**Independent reviewer, Rust-only:** producer SHA-256 `814c6373c78aad5acbb57a6c70084eb0b18a43fe3acadf5ee0207218f10172aa`, `--no-default-features --features lang-rust`, Rust/Cargo 1.95.0, 1 job, incremental off, debug 0. The exact-head integration test passed 6/6; Linux native-path tests passed 5/5. The independent native fixture exercised 15 subjects, and 10 isolated controls generated 30 passing CLI reports. This is separate from the author default-feature lane; no Windows-native claim is made.

**Focused exact-published-SHA unit recovery:** on `f6aab7639b88be1a21c757be74baef833e636f17` with default features, Rust 1.95.0, 1 job, incremental off and debug 0, the two named full/compact receiver unit tests passed (2 passed, 10,814 filtered, exit 0; 339 seconds). This is a bounded targeted run, not a full lib-test-suite pass.

## Incomplete checks and inherited failure

A prior default-feature lib-test compilation at the reviewed local identity was SIGKILLed (exit 101) before completing; the default `check-fast` aggregate also exited 1 when all-target Clippy's lib-test metadata process received SIGKILL. The focused published-SHA run above now establishes the two targeted unit assertions only. It does not turn the interrupted aggregate into a full-suite pass.

The exact-head golden check ran 425 fixtures with zero fixture errors and exited 1 on the inherited `owner_return_pin_fallible_literal` fixture (three output surfaces). The immutable baseline producer at `89ac47a…` and candidate producer produce identical normalized JSON for that fixture after the normalizer removes `canonical_next_action`; no candidate-new drift was observed. No expected output was blessed. The failure remains visible.

## Bounded claim and residuals

The change is a deny-only refinement for parser-established inherent methods with `self`, based on nominal receiver plus real call syntax and a nearest still-visible simple binding. It does not establish exact Cargo-target or complete Rust type identity. Qualified type-name collisions, constructor assumptions, aliases/factories/parameters, general value flow, nested/custom/file-scope macro expansion, arbitrary target roots, and general helper last-hop receiver identity remain limitations; unsupported evidence may remain unknown. The P2 memoization suggestion in the review is source-derived only and has no measured performance result.

#7415 remains open. This packet does not establish Ready/merge clearance, current-base qualification, hosted-check completion, or a fresh CLI build at the published SHA. See `public-handoff-draft.md` for a concise comment draft. `manifest.json` records the exact bundled-file and omitted-output digests; omitted executables and report dumps are intentionally not embedded.
