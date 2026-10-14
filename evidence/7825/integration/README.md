# Local composition and focused compatibility handoff for #7825

## Exact trees

The Draft PR head remained `54e73e4b0fde7297203fb42a729e4ce621514e0c` (tree `c9f068b2a1340a06a3572768e66f31c5b68f17c6`); both the branch and `refs/pull/7923/head` resolved to that SHA after validation. No PR branch update was made.

Latest current-main composition: commit `9aede4722bf4776895ccf1aa10e9c187ffd976a1`, tree `7457f5b00310eca793c38ae616518cd31e31fa12`, parent 1 current `main` `926f50127fa167daefb534a81146e4b7a262e561`, parent 2 exact PR head `54e73e4b0fde7297203fb42a729e4ce621514e0c`. It was created in a disposable worktree. The merge was clean. The intervening main commit from `bbdbae9332828ceb2729f6238fa5465164b6c267` to `926f50127fa167daefb534a81146e4b7a262e561` touched only `crates/ripr/src/cli/commands/gate.rs`, SPEC-0014, and the #7741 changelog.

## Latest-composition validation

Commands ran with locked dependencies, the repository-pinned Rust toolchain, and the shared existing Cargo target directory:

- `cargo test --locked -p ripr --test cli_smoke sample_output:: -- --nocapture`: 2 passed, 513 filtered. This runs the real-CLI wrong-output/refusal/schema/count mutation controls and the parallel private-state/cleanup control; expected panics from deliberately mutated outputs are caught by the test.
- Each of the six preserved top-level smoke symbols was invoked separately with `cargo test --locked -p ripr --test cli_smoke <symbol> -- --exact`: 1 passed for each, 514 filtered. These execute the actual Cargo-built CLI through private fixture roots.
- `rustc --edition 2024 benchmarks/cli_sample_output/check_reads.rs -o /tmp/7825-check-reads-latest`; the checker passed all 12 archived candidate and 64-file-growth traces (six each), with exactly two owned Rust source inputs per trace. The descriptor-only sibling-root substitution control was rejected because its returned Rust read was outside the CLI-selected fixture.

Saved command logs are in `logs/` alongside this receipt.

The four source consumers `cargo xtask check-process-policy`, `cargo xtask check-traceability`, `cargo xtask check-fixture-contracts`, and `cargo xtask check-output-contracts` all exited zero on the preceding clean composition `0ebd425a9593108f5d2e1020eadce614feec9928` (tree `8a954751ef5a44e217d2d0602c515f5aa0f9e59e`, parent 1 `bbdbae9332828ceb2729f6238fa5465164b6c267`, parent 2 the same PR head). The later `main` delta changes none of these checker, fixture, smoke-test, or traceability paths. Independent exact-tree review confirmed all six traceability mappings, both fixture-control mappings, the retained main-side `closed_stderr` registration, and the sample-output module registration at the latest tree.

## Review and limits

The unchanged source review at final PR head `54e73e4b0` was reused. A fresh integration-seam review of latest composition `9aede4722` found no actionable interaction and verified both parents, tree, remote PR head, traceability entries, and module registrations. This is source and local integration evidence, not hosted check qualification or native Windows qualification.

No benchmark was repeated. The before/after measurements remain the recorded local observations in the raw-witness archive. GitHub still reported the PR open and Draft; its API returned `mergeable=false` during this handoff. No Ready, merge, release, deployment, security, permissions, gate-threshold, or protection operation was performed.
