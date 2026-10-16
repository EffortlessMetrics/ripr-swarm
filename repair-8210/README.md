# #8210 native partial-preview disclosure repair

Candidate: `67dc5af9a15590fc8476d09217232831a0009952`, tree `87daa9ea61cb717b743733e0734346702573e001`, basis `d08c77d144c016078364fcc1fcf2fa593d21cefc`. Branch `codex/8210-partial-preview-disclosure`. This is an author proof handoff, **REVIEW_INCOMPLETE** pending parent independent review; no Ready/merge/hosted or fanout qualification.

## Change and authority

The native pipeline now applies its existing selected-partition predicate to enabled source-adapter advisory paths. An entirely omitted Python/TypeScript language has no analyzed advisory; a partly selected language has the selected count/sample paths. Disabled presence/enablement guidance remains raw. Perl caller-supplied packet scope remains unchanged. Existing renderers consume that producer value: no renderer-local routing, classifier, partition, cache, exit, policy, feature or schema-version change. Seven source/evidence/docs paths; only 11 production lines, all in the advisory-construction seam.

#8201 retains fanout ownership. Its exact observed pipeline patch adds only a cfg(test) entry counter, away from this advisory seam; `current-source.json` retains that patch and the live source read. Coordination was published and read back at https://github.com/EffortlessMetrics/ripr-swarm/pull/8201#issuecomment-6209124827. No fanout dispatch, custody, output-set implementation or fanout test changes.

Main advanced to `e660a99480dc13a4ed93b131042d11506fd484b6` during proof. Its 31 changed paths do not overlap this candidate's seven paths. The live pipeline blob is still `370c3e8f12d11d26fc3ff18b0e88860c2c5bf4cd`, with the original defect; the current human advisory renderer still consumes it. Relevant primary readbacks and delta are in `current-source.json`. No automatic rebase: all proof is on the frozen candidate/basis; combined-tree qualification is not claimed.

## Observed proof

Exact-base before mutation: worktree doctor exit0; check-fast six applicable gates, zero paths (independently matching Git at the basis); `cargo check --locked -p ripr --all-targets` exit0. Logs prefixed `base-`.

Intended red: the new real CLI regressions failed on both Python and TypeScript omitted-preview assertions before the production repair, Cargo101. `red.log` is the behavioral witness, with unchanged production source. The initial shaping green attempt instead revealed a wrong test expectation for selected-file ordering at budget2: native partitions list Rust first. That literal test expectation was corrected; no product order changed. `green.log` is retained as a failed shaping run and not counted as proof; its format-tail overlap also excludes it from final qualification.

At final head, 19 focused tests passed:

- `cargo test --locked -p ripr --test cli_smoke partial_preview_disclosure`: 2 tests, 20 native check invocations. Python/TypeScript budgets1/2/3 cover wholly omitted, partly selected and complete source; disabled controls retain detected-file guidance. Literal selected paths, nonempty Rust/preview subjects, analyzed counts/samples, raw input counts, complete/partial outcomes, non-consumability, gate ineligibility and native exit0 precede human wording assertions.
- `... unsupported_check_exit`: 4 passed.
- `... python_source_admission`: 11 passed.
- `... findings_byte_budget`: 2 passed.

`cargo xtask check-fast`: native0, 14 gates including workspace/all-target Clippy with `-D warnings`; seven selected paths agree with successful independent Git diff and basis. It skipped its separate fixture-contract gate; `precommit` subsequently passed it. Selector and raw log retained.

`cargo xtask precommit`: native0, including output contracts, fixture contracts, changelog format, process/network policy and other listed checks. `cargo xtask check-traceability`: native0. Traceability validates authored paths/coverage, not Rust symbol roles; the two named new tests were separately observed executing.

Six bounded fixture checks each native0, no golden blessings: `boundary_gap`, `python_boundary_gap`, `typescript_boundary_gap`, `python_disabled`, `typescript_disabled`, `typescript_parse_error_unsupported_syntax`. Actual normalized outputs are under `fixtures/`; logs retain the supported runner's build and terminal results.

All 13 final command statuses/argv and head bindings: `validation-receipt.json`. Source and executable identities: `evidence.json`.

## Native comparison

`native-compare.py` executed 70 bounded standalone native invocations: 29 unaffected before/after stdout pairs are byte-identical (also matching stderr/exits); four mixed-partial JSON/human pairs differ only by the intended omitted advisory; four JavaScript controls retain omitted versus selected disclosure. For scoped JSON, removing only the old preview_languages field makes the entire documents equal; for scoped human, removing only the exact old Note line makes bytes equal. Partial analysis remains incomplete/non-consumable, and badge bytes remain unchanged. Rust/Python/TypeScript complete, disabled, malformed and JSON render-limit cases are included. Raw streams under `native/`; exact argv/env/status/hash receipt `native-compare-receipt.json`.

The final selected comparison executable is the actual Cargo test-built binary copied into an exclusive real-file snapshot `/workspace/scratch/8210-proof/67dc-ripr`, 0.11.0 at full final SHA, SHA256 `285539d6728ca3e2b50a14d348e72a8e4287db2a26d4d6028090a7467fe35398`. Original base snapshot SHA256 `73441c38399894fd6c21f9dd1893d4eb90505b54f509033233afac781d29591c`. Binaries are retained locally, not uploaded or represented as released artifacts. The later fixture runner performs its own supported `cargo build`; its receipt establishes fixture behavior at the same source, not byte identity with the frozen comparison executable.

To reproduce, check out the exact candidate and use the commands above. The fixed native comparison cases are retained in the earlier immutable audit artifact `8c43a954db38536d85fd7827add8ec2f5074949e/cases/`; JavaScript construction is in the script and its resulting input in `js-case/`. Adjust script source/proof paths and set `FINAL_BINARY` to the independently built exact-source executable. Inspect all JSON bytes and process statuses. `final-control.rs` is an exact final test snapshot; `partial_preview_disclosure.rs` is the original pre-repair test stimulus snapshot. `451d-*` receipts and the intermediate binary identity are superseded shaping evidence only.

## Challenge, limitations, and continuation

Literal positive and disabled controls reject deleting all advisories, deleting every partial advisory, shrinking only the count while retaining stale sample paths, falsely completing a partial run, or losing Rust findings. The original behavior was rejected by the unchanged omitted-scope assertions; the repaired native path is observed. The source challenge explicitly preserved packet-scoped Perl semantics. No independent author review is claimed.

Linux/default-feature execution only. No full workspace test/golden/dogfood suite, Windows, nondefault feature execution, hosted integration, package/release or fanout candidate proof. No semantic support promotion or gate change. The actual remaining transition is parent independent exact-published-head review, then any authorized integration proof. Keep Draft. No denied operation or pending permission blocker; no Ready, merge, thread resolution, bot contact, deployment, spending, security change, destructive cleanup or workflow weakening occurred.

Storage and retained-resource accounting: `resources.md`. All owned validation drivers are terminal; source, frozen witnesses and mutable build state remain retained for review/recovery. The >=10 GiB free-space reserve was preserved; no cleanup was used to admit builds.
