# #8135 current-main integration qualification

Local composition qualified; HOLD_READY for stale hosted merge subject. No submitted branch changed. Parent owns Ready, review-thread resolution and merge. #8142 remains the precision alternative; #8150 remains its owner's separate root-coverage improvement.

| Identity | Exact object |
| --- | --- |
| Source #8135 | 7d246801183e22bcf78e6c2efd556e387e27b1e2 |
| Source tree | 10fbc4170b485e97d83e30d8d759254bc66777a2 |
| Current main basis | 758a7c082c1c140d1d818bb3d7e5be78643d3f22 |
| Executed isolated composition | 05e332b66af067a1a365f4d1f5af18e32fd925f2 |
| Executed tree | 48bf287c742186117177822ae098a8bbe26fbfa9 |
| Composition parents | current main basis + unchanged source above |
| Material landed prerequisite #8102 | 98c8f172bfd6aa2b808f63431ed4eac9c224c102 |
| Executed prerequisite fixture blob | 3ebc6faf435a443c6baa31bf410e53d121cadf6f |
| Observed hosted merge | a8bc5c7439916f7b95b48ed4e961828caf50b725 |
| Hosted merge actual base/tree | ef12e0258a63e2476fcf9509d867152d6b36df80 / 39d150d9aeffefc32ff0e0b82221b153b62b10c0 |
| Hosted prerequisite fixture blob | ff7f92cfb100a6a674d3d4214f67bb21f87c3bcd (old; missing guard) |

Observed local commands on the clean executed tree, all exit0:

- `cargo test --locked -p ripr --lib output::gap_decision_ledger`: 116 passed.
- `cargo test --locked -p ripr --lib related_tests::verify_command_tests`: 5 passed.
- `cargo test --locked -p ripr --lib lsp::diagnostics::seam_diagnostic_tests`: 31 passed, including all 11 required gap diagnostics named in verification.json.
- `cargo test --locked -p ripr --no-default-features --features lang-rust --test cli_smoke structural_workspace_markers`: 3 passed. Rust-only unused/dead-code warnings are retained. The uv control asserts refusal without lang-python; it is not the stronger successful uv analysis/root/probes proof owned by #8150.
- `cargo xtask check-fast`: pass, 13 gates, independently verified stable 4-path selector against exact current main. Includes strict `cargo clippy --workspace --all-targets -- -D warnings`. Support-tier and fixture-contract checks skipped as inapplicable.
- `cargo xtask precommit`: pass, local_precommit on exact composition/base, same four change records, selected ripr/xtask all-target Clippy passed, no recorded skips/limitations. Independent report digest: sha256:b6bcb4c4179f28bcda7ab5679458fa44d37eaa6fe60ca72a1bccc91a5b1e271e.

The original driver/session19600 completed exit0. Disconnect notification did not interrupt it; no completed behavioral run was repeated. A wrong diagnostic module filter selected zero subjects, retained in diagnostic.log/json as non-evidence; the discovered module was then run once. Initial toolchain-path setup error and independent review's exact gh Forbidden read are retained, with no credential/security changes or denied-route retry.

Independent source/integration review is in independent-review.md. The three main deltas since the prior ef12 basis were inspected; Python selector/card/JSON/ledger consumers retain their reviewed bytes. Candidate production/tests are unchanged; only integration-basis currentness required refresh. No blocking source/integration finding. Original capped-row P1 remains repaired; optional namesake precision and cleanup-on-panic stay nonblocking. Historical source review https://github.com/EffortlessMetrics/ripr-swarm/pull/8135#pullrequestreview-5536614917 remains applicable. Prior archive81 hashes are referenced at https://github.com/EffortlessMetrics/ripr-swarm/tree/3b0c45ed55e70bab1ed5a0941ab592aa340ad5b1, not claimed freshly restored here.

READY-DISPOSITION.md gives the parent-owned resume condition. Fresh final refs still show source/head unchanged, main at 758a, and GitHub merge ref at a8bc. The historical hosted Rust-only job116522145229 is FAILED; local green does not change it. The required Ready-triggered Ripr Rust Small Result is absent on this Draft head, not passed. Verify corrected actual remote parents/tree/prerequisite bytes before Ready, then inspect the actual admitted checkout and terminal required checks before merge. No behind-only restack is justified by this local proof. A necessary submitted branch change must first be reported to parent.

Raw commands, native terminal statuses, logs, policy reports, environment, blob bindings, API/ref receipts, independent review, full integration patch and a Git composition bundle are retained. Run `python3 verify-receipts.py` to recompute policy digest and verify command counts/mandatory diagnostic subset; `sha256sum -c MANIFEST.sha256` verifies packet bytes. The composition bundle requires a142074061128a85e76c1ac494b9939f3a58428b and 758a7c082c1c140d1d818bb3d7e5be78643d3f22 already available from repository history; its advertised HEAD is the exact tested composition.

No hosted aggregate clearance, Windows/macOS, broad fixture/golden/dogfood, live pytest/editor, external accuracy, release or publication acceptance is established. Retained-reference judged-panel differences in precommit remain reference-policy observations, not fresh candidate accuracy evidence. All three review threads remain unresolved and unchanged for parent disposition/resolution. No Ready/merge/closure, bot contact, release, deployment, spending, credential/security/settings change or destructive cleanup was performed.
