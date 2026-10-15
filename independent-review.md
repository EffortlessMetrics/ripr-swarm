# PR #8135 independent integration source review

No blocking source or integration findings emitted. The prior source review remains applicable to the inspected Python ledger behavior. This is a Draft handoff, not REVIEW_READY.

reviewed_head_sha: 7d246801183e22bcf78e6c2efd556e387e27b1e2 (unchanged PR source)
reviewed_integration_sha: 05e332b66af067a1a365f4d1f5af18e32fd925f2
reviewed_integration_tree: 48bf287c742186117177822ae098a8bbe26fbfa9
integration_basis: 758a7c082c1c140d1d818bb3d7e5be78643d3f22; integration parents independently read as this basis plus the PR head
previous_basis_compared: ef12e0258a63e2476fcf9509d867152d6b36df80
review_environment: Bash/Linux, /tmp/ripr-8135-integration, 2026-10-15 UTC; clean checkout before/after read-only inspection; toolchain file pins 1.95.0
claim_boundary: Python check-JSON -> gap ledger target_line must belong to the card-selected runnable test; withhold precision when related inventory is incomplete, lossy, ambiguous, unmatched or invalid. No selector, command, gate authority or general navigation redesign.

## Observed source and integration evidence

Read root AGENTS.md, review-pr procedure, review guidelines/rules, engineering/architecture/scoped contract, review invariants/validation, relevant CI and Draft-to-Ready contract, Python preview spec and traceability. Inspected full PR delta (4 files, 783 insertions/1 deletion), the new tests, semantic producer/selector and relevant consumers.

Integration has byte-identical PR production/test blobs:
- gap_decision_ledger.rs: 63f5d736dd30fd7f2091a2b695f00d79b6e8308f
- gap_decision_ledger/python_identity_tests.rs: bbe9b9775e8d5e61ad60a7dc79d115d970f8b15f

Relevant prerequisite/consumer blobs are identical between ef12 and this integration:
- analysis/language/python/related_tests.rs: 2ba6a0d5dac28181d561e7126ed71d62df8243f6
- related_tests/verify_command_tests.rs: 8caed3422458e6008c37d7204b3b810aaa6d6201
- output/json/report.rs: 5caea7f2ad59b936667a1ba8baa17012865427a1
- output/python_repair_card.rs: 1777f2d76c69d325cd2c1da3c43fcd17bf8cbfc8
- output/agent_seam_packets.rs: c898c9a3922c264d5be2951e48a0f86a40cc80df
- lsp/actions.rs: dffdbb61c3990fe86de65f4e4dcb82405d0551ab

The three intervening main commits are a184e8145 (TypeScript mock factory), 98c8f172b (#8102 expect_file Rust owner pins/cache versions plus Rust-only uv refusal characterization), and 758a7c082 (TypeScript colon __proto__). Inspected changed path inventory, TS production guards/types/classifier, shared Rust reveal/cache changes, schema edits and uv characterization. None changes Python selected identity, check-JSON cap/total, ledger mapping or retained consumers. Rust cache/schema version changes and TS oracle changes are real independent semantics, not evidence of Python drift. No workflow/dependency/platform-production changes in this PR. The uv prerequisite is present at blob 3ebc6faf435a443c6baa31bf410e53d121cadf6f in the reviewed integration.

## Correctness and oracle challenge

The selector skips unrunnable dotted unittest paths and selects the first runnable oracle relation. JSON emits canonical file/name rows, pre-cap related_tests_total and at most eight rows. The new mapper requires exact nonblank file/name, total == visible count, intact identities on every row, a unique selected match and a positive integer line. It cannot borrow the first unrelated line or a visible namesake hidden behind a cap. Refusing capped/lossy precision leaves the selected target file/name, command and full record intact. Existing scalar consumers allow optional target_line; no schema shape change is needed.

Read the real-parser/check-renderer test construction and complete-record literal oracle, including selected-second line60 versus unrelated line10, selected-first, reversed rows, duplicates, missing/malformed/zero line, absent/invalid totals, lossy unselected identities and real cap/order/namesake cases. The historical capped counterexample is retained in source: selected pytest line28, visible unittest namesake line25, total9/rendered8; the mapper now withholds target_line. Restoring first_related_test_line would contradict the selected-second oracle; removing completeness/uniqueness checks would contradict the capped and duplicate controls. This pass did not execute those controls, by delegated no-Cargo boundary.

## Findings and limitations

blocking_findings: none found in inspected source/integration delta.
non_blocking_suggestions: prior optional namesake precision and panic-safe test-temp cleanup remain nonblocking; no duplicate repair proposed. Same-file/name ambiguity deliberately withholds a line despite richer internal placement evidence. Separate #8150 Ready-test precision work was not duplicated or incorporated.
refuted_or_stale_findings: prior capped-row P1 is repaired in the inspected mapper and covered by the retained line28/line25 source counterexample.
affected_currentness_dimensions: integration basis refreshed; source PR implementation, stimulus, oracle, public claim and generated relationships remain unchanged. No conflict-specific edits found in those bytes. Candidate identity remains bound to 7d246801; local integration identity is distinct from hosted merge identity.
proof_and_ci_observed: git object/parent/tree/blob comparisons; complete relevant source and test inspection; git diff --check passed. No Cargo run or hosted CI execution observed by this reviewer.
reported_by_parent: prior review5536614917 and prior test receipts; earlier hosted a8bc5c74 Rust-only run failed at uv (477 pass/1 fail); current remote merge ref still a8bc5c74 and missing #8102. These are parent-provided context, not independently retrieved hosted evidence in this pass.
proof_or_review_missing: current composed-tree runtime results are owned by parent; current hosted checks, threads and merge-ref identity are not established by this reviewer. A single attempted gh API read of review5536614917 returned Forbidden; no retry, broad access conclusion or bot contact. Historical merge a8bc5c74 was absent at initial inspection; the follow-up below supersedes that temporary limitation after the parent fetched it.
residual_assumptions_and_non_claims: static source review does not establish runtime pass, Windows/macOS qualification, remote merge-ref correction or required hosted checks. Existing LSP navigation may fall back to diagnostic anchor when optional target_line is absent; this unchanged consumer behavior is outside the PR's ledger identity claim and this pass does not certify its UX.
disposition: REVIEW_INCOMPLETE / Draft handoff. Source review supports the unchanged PR on this local composed integration, with no blocking finding. Parent must bind completed runtime receipts and resolve/verify the material hosted-subject gap under docs/PR_AUTOMATION.md before Ready; Ready, threads and merge remain parent-owned.


## Follow-up: CLI read limit and fetched historical integration

The exact attempted Bash command was:

```bash
gh api repos/EffortlessMetrics/ripr-swarm/pulls/8135/reviews/5536614917 && gh pr view 8135 --repo EffortlessMetrics/ripr-swarm --json headRefOid,body,state,isDraft && gh issue view 8132 --repo EffortlessMetrics/ripr-swarm --json title,body,state
```

It ran with workdir `/tmp/ripr-8135-integration`. Native exit status: **1**. Full returned output:

```text
Get "https://api.github.com/repos/EffortlessMetrics/ripr-swarm/pulls/8135/reviews/5536614917": Forbidden
```

The first API read failed; Bash `&&` prevented the following PR and issue reads from running. This establishes only that particular `gh` CLI request's denial, not a general GitHub/API/connector access failure. No retry was attempted. Parent reports it had already retrieved the prior review through the connected GitHub tool before this CLI attempt; that independent connector evidence is parent-owned.

After the parent fetched the historical merge into the common object store, independently inspected with `git show -s`, `git ls-tree`, and `git diff`:

- Historical commit: `a8bc5c7439916f7b95b48ed4e961828caf50b725`.
- Parents: `ef12e0258a63e2476fcf9509d867152d6b36df80` and unchanged PR head `7d246801183e22bcf78e6c2efd556e387e27b1e2`.
- Historical tree: `39d150d9aeffefc32ff0e0b82221b153b62b10c0`.
- Historical structural-marker blob: `ff7f92cfb100a6a674d3d4214f67bb21f87c3bcd`.
- Composed structural-marker blob: `3ebc6faf435a443c6baa31bf410e53d121cadf6f`.
- Their structural-marker diff is exactly the 13-line Rust-only uv refusal branch: when lang-python is disabled, require unsuccessful status and both workspace-marker/feature diagnostic strings, then continue. With lang-python enabled the existing success/root assertion path remains.
- Historical-to-composed tree delta is the same 34-file/1275-insertion/141-deletion intervening-main inventory already assessed above; the PR Python mapper/test bytes remain unchanged.

This confirms the historical local object lacks the relevant uv characterization and the new composed object contains it. It does not independently establish the current hosted merge ref. Re-read of the inspected checkout still shows clean status, commit `05e332b66af067a1a365f4d1f5af18e32fd925f2` and tree `48bf287c742186117177822ae098a8bbe26fbfa9`.

No Cargo execution or runtime receipt was inspected during this follow-up. Parent-reported ledger116, selector5 and structural Rust-only3 passes remain reported evidence here. The parent-reported wrong-module diagnostic filter selected zero subjects and is not proof. Runtime/hosted limitations and the REVIEW_INCOMPLETE / Draft handoff disposition remain unchanged.
