## Summary

Workflow and agent-skill policy currently start through xtask's product/parser build. This extracts their existing shared implementation into unpublished `repo-policy` and runs both checks before product allocation. The two maintained CI callers verify a source/compiler/input-bound preflight receipt at precommit's original workflow-check position, preserving the remaining table while executing each extracted check once.

Closes #7158. Parent/controller: #1029. Existing authority: `docs/ci/PRODUCT_GATE_PLAN.md` and repository policy implementations.

## Swarm / Source Boundary

- [x] swarm-only CI, routing and agent-operating surface
- [x] One architectural seam: the workflow/agent policy bootstrap

Candidate: `codex/issue7158-policy-bootstrap`, exact head `d902d09922245fb47e9af5a941022ff7da86c8ea`; no stack parent or replacement. Input base `331176f6cf6405ba9688b7a5dbca6654453eaad1`. Current integration main `9cb17ebf415164fce6ee74c6196afd2a630bd56b` merges cleanly; combined-tree execution remains hosted proof, not inferred from mergeability.

## Production and evidence delta

- One shared Rust implementation; existing xtask commands delegate and `cargo policy` directly uses it. Normal/build dependency and cold compiler traces exclude `ripr`, RA and OXC.
- `.github/workflows/rust-gates.yml` and the routed docs gate run `cargo policy preflight`, then complete `cargo xtask precommit` with receipt verification. Default local precommit still runs its full table.
- Retained executables reject stale source/compiler identity. Preflight invalidates an earlier receipt before validating the producer, and missing/failed/stale/changed-input receipts fail closed.
- Existing command catalogue/product-plan/allowances migrate; helper-only Rust edits select Rust gates. The new helper manifest has an exact existing file-policy entry, not a broad allowance.
- 54 extracted bodies preserve their behavior; existing workflow/agent/scratch tests plus real frontdoors and retained-baseline comparisons exercise success, contract violation, missing/unreadable input and receipt boundaries.

Non-goals: whole docs/precommit product independence, profile tuning, feature/test removal, cadence/runner changes, queue activation, product semantics, issue7251 or PR7249 work. Public product CLI/JSON and support tiers are unchanged. The separate [target guidance branch](https://github.com/EffortlessMetrics/ripr-swarm/compare/331176f6cf6405ba9688b7a5dbca6654453eaad1...codex/issue7158-target-guidance) is not included in this patch.

## Acceptance and proof

| Acceptance | Retained evidence |
|---|---|
| Existing positive/negative behavior through real frontdoors | 34 library +4 frontdoor tests; six old/new executable comparisons with identical exits/stdout/stderr/reports |
| Real product-independent build | Separate fresh cold workflow/agent targets:21 compiler-artifact units each, no product/parser graph |
| One execution in actual migrated caller | Final preflight runs both once; actual Cargo-selected final xtask precommit verifies receipt without repeating either |
| Fail-closed retained producer and receipts | Changed implementation/tree, ignored input, missing/failed receipt tests; old/final CLI differential observes repaired stale-receipt removal |
| Reconciled retained consumers | 7 workflow-contract tests,3 catalog tests and strict selector control; unchanged subject blob identities independently verified |
| Remaining repository obligations reachable | Final complete precommit and workspace/all-targets Clippy pass; workflows retain nextest/doctest/features/golden/fixture obligations |

Final-head commands/results:

- `cargo test -p repo-policy --locked --offline`:34 library +4 integration tests passed.
- `cargo clippy -p repo-policy --all-targets --locked --offline -- -D warnings`:pass.
- `cargo build -p xtask --locked --offline --message-format=json`:pass; selected artifact frozen and hashed.
- `cargo policy preflight`:native0,8.7185s, source-bound receipt.
- Compiler-selected final `xtask precommit` with receipt: native0,1398.3903s; emitted exact-head precommit-v2 report includes workspace/all-targets Clippy pass.
- Same final `xtask check-fast`:native0,136.0642s; independently reconciled51paths against retained then-origin60290d6e,14gates run/1skipped. Clippy fresh0.32s.
- Exact-base retained `xtask check-fast`:native0,28.1947s under1800s overall bound. Refreshed main descends from the input base, so independently verified zero selected paths is expected;6always-run gates passed,5conditional gates skipped, no product compilation.

The [immutable proof packet](https://github.com/EffortlessMetrics/ripr-swarm/tree/d4894a48b86acd94ab45c7a27efb403ba516bfe2/issue7158-proof) contains final helper/caller proof; a follow-up packet adds baseline completion and final integration review. Source followups and fresh final scoped review found no blocking source/oracle defect. Disposition is `REVIEW_INCOMPLETE` pending exact published-head hosted checks and substantive merge review. Earlier native143 interruptions, instrument timeout and repaired manifest failure are retained as non-passes.

## CI Economics and limits

One-job/offline bootstrap measurements with populated download cache: workflow cold21.3675s/warm1.9582s; agent cold10.7660s/warm0.1902s. Cold21/warm0 newly compiled units. Actual policy stdout57/63bytes is separate from approximately18.7KB compiler JSON and Cargo stderr. Baseline all-targets cargo check compiled268units in3m37s; different scopes do not establish a total-CI speedup ratio.

Existing early administrative signal becomes smaller; required contexts and feature/release obligations stay reachable. No new workflow, broad label, settings or manual dispatch. Artifact additions: policy preflight receipt; existing workflow/agent report semantics preserved. Receipt currentness assumes a private trusted writable target; it is not producer authentication against another writer.

The1398.39s full precommit,136.06s check-fast,fresh Clippy repetition and absent overall driver deadline are observations for existing efficiency owners, not extra changes here. File-policy compile/list guards remain30/5minutes. Resource recovery retained all source/unique proof/witnesses/shared caches and reclaimed only inactive task-owned reproducible output under admission.

## Rollback and engineering

Revert the extraction and caller/dependency/allowance wiring together, restoring the former delegates/call sites. No branch-protection or release settings change is needed. Tests/metrics map directly to #7158's operational acceptance; no product spec/golden change.

- [x] Existing lint/no-panic/file/dependency/process policy checks pass.
- [x] `cargo fmt --check` and workspace/all-targets Clippy pass.
- [x] Remaining precommit policy checks pass.
- [ ] Full hosted nextest/feature/doctest/golden/fixture/platform/package matrix: not claimed locally; retained at existing lifecycle boundaries.

Publish as Draft only. Ready and merge require separate approval.
