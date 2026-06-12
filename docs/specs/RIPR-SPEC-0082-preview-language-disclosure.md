# RIPR-SPEC-0082: Preview-Language Disclosure

Status: proposed

Owner: product / swarm

Created: 2026-06-12

Linked proposal:

- None yet

Linked ADRs:

- None yet

Linked plan:

- None yet

Linked issues:

- #1111 — Silent "No probes found" honesty gap for preview-language content

Linked PRs:

- None yet

Support-tier impact:

- No tier change. This spec adds advisory disclosure output when TypeScript,
  JavaScript, or Python files are in the analyzed scope and the repo has opted
  in to those adapters. It does not promote any preview adapter to a higher
  support tier, does not change pass/fail authority, and does not alter what
  the adapters classify.
- The disclosure is additive output only. Claim boundaries remain governed by
  the canonical ledger in [support tiers](../status/SUPPORT_TIERS.md).
- Empty-result semantics remain unchanged: "No probes found" still means the
  static analyzer found no mutation exposure probes, not that the diff is
  safe.

Policy impact:

- Register this spec in `policy/doc-artifacts.toml`.
- No new crates, binaries, dependencies, parsers, runtime executors, or LSP
  servers introduced by this spec.

## Problem

When a user enables a preview language adapter (TypeScript, JavaScript, or
Python) and runs `ripr check --diff`, an empty result is ambiguous. The output
says "No diff-derived mutation exposure probes found" but gives no signal that
the diff contained preview-language files. A reader cannot distinguish between:

1. The diff contained no preview-language content, so nothing was in scope.
2. The diff did contain preview-language content, but the preview adapter could
   not classify it (empty-class probe, incomplete parser support, etc.).

Case (2) is a silent honesty gap: the preview adapter is advisory and may be
incomplete, so an empty result from a TypeScript diff is NOT a clean Rust-grade
result. Without disclosure, operators may incorrectly conclude the diff is fully
analyzed.

## Behavior

### Detection

When the analysis pipeline runs with one or more preview-language adapters
enabled (via `[languages] enabled` in `ripr.toml`), it counts the files in the
diff that route to each preview adapter using the same path router
(`analysis::language::route`) that dispatches to adapters. The count is real
— it comes from routing actual diff paths, never fabricated.

A `PreviewLanguageAdvisory` is produced for each preview language where
`file_count > 0`. Each advisory carries:

- `language`: stable wire string (e.g. `"typescript"`, `"python"`)
- `file_count`: number of files in the diff that routed to this adapter
- `sample_paths`: up to three normalized file paths (forward-slash)

Advisories are propagated through `AnalysisResult` → `CheckOutput`.

### Human output

When any advisory is present, a `Note:` line is appended after the findings
(or after the "No probes found" line for empty results):

```
Note: 1 Typescript(s) analyzed under preview support — preview evidence is advisory and may be incomplete. An empty result here is NOT a clean Rust-grade result.
```

The note is omitted entirely for pure-Rust diffs. The note does not change
exit code or pass/fail status.

### JSON output (`--json`)

When any advisory is present, an additive `preview_languages` array is emitted
after `findings`. It is absent when the array would be empty (pure-Rust scope).
No schema version bump is required per the additive field policy in
[`docs/OUTPUT_SCHEMA.md`](../OUTPUT_SCHEMA.md).

Example:

```json
"preview_languages": [
  {
    "language": "typescript",
    "file_count": 1,
    "sample_paths": ["src/utils.ts"],
    "category": "preview_language_advisory",
    "why": "preview adapter; advisory; may be incomplete; empty result is not Rust-grade clean"
  }
]
```

### Non-claims

- This spec does NOT fix gaps in TypeScript/JavaScript probe classification
  (e.g., `jest test()` / `expect()` detection). That is separate work.
- This spec does NOT change what the preview adapter analyzes or classifies.
- This spec does NOT promote TypeScript, JavaScript, or Python to a higher
  support tier.
- This spec does NOT change the exit code or the pass/fail gate authority.

## Non-Goals

- Automatic preview-language detection without `ripr.toml` opt-in.
- Per-file disclosure granularity beyond `sample_paths`.
- Disclosure in SARIF, GitHub, badge, or repo-exposure output formats.
- Fixing TypeScript `jest test()` / `expect()` probe detection gaps.
- Runtime mutation testing, coverage measurement, or correctness claims.

## Required Evidence

- Diff changed-file list from `analysis::diff::parse_unified_diff`.
- Language router output from `analysis::language::route` for each changed path.
- `ripr.toml` `[languages] enabled` configuration with at least one preview
  language (TypeScript, JavaScript, or Python).

## Inputs

| Input | Required? | Purpose |
| --- | --- | --- |
| `ripr.toml` `[languages] enabled` | yes | Controls which adapters run; disclosure fires only when a preview adapter is enabled and finds files |
| Diff changed-file list | yes | Provides paths routed to preview adapters to count files and collect sample paths |
| Language router (`analysis::language::route`) | yes | Real adapter routing — same predicate used for dispatch |

## Outputs

| Output | Schema impact | Notes |
| --- | --- | --- |
| Human text `Note:` line | None | Additive; absent for pure-Rust scope |
| JSON `preview_languages[]` | Additive field | Absent when empty; no schema version bump |

## Acceptance Examples

1. Diff contains `.ts` file, `ripr.toml` has `enabled = ["typescript"]` →
   human output includes `Note: 1 Typescript(s) analyzed under preview support`.
2. Diff contains `.ts` file, `ripr.toml` has `enabled = ["typescript"]` →
   JSON output includes `preview_languages` array with `file_count: 1`.
3. Diff contains only `.rs` files → NO `Note:` line, NO `preview_languages`
   field in JSON output.
4. Diff contains `.ts` file without `ripr.toml` (or with only `enabled = ["rust"]`) →
   NO disclosure (TypeScript adapter not running).
5. Count in `Note:` matches `file_count` in advisory, matches files routed by
   `analysis::language::route` to that adapter.

## Test Mapping

- `crates/ripr/src/output/human.rs::tests::render_emits_preview_disclosure_when_typescript_files_in_scope`
- `crates/ripr/src/output/human.rs::tests::render_emits_preview_disclosure_when_python_files_in_scope`
- `crates/ripr/src/output/human.rs::tests::render_omits_preview_disclosure_for_pure_rust_scope`
- `crates/ripr/src/output/human.rs::tests::render_preview_disclosure_count_matches_advisory_file_count`
- `crates/ripr/src/analysis/pipeline.rs::tests::spec_0082_diff_pipeline_emits_preview_advisory_for_typescript_files`
- `crates/ripr/src/analysis/pipeline.rs::tests::spec_0082_diff_pipeline_emits_no_preview_advisory_for_rust_only_diff`
- `crates/ripr/src/output/diff_report.rs::tests::diff_report_includes_preview_languages_when_ts_files_in_scope`
- `crates/ripr/src/output/diff_report.rs::tests::diff_report_omits_preview_languages_for_pure_rust_scope`

## Implementation Mapping

- `crates/ripr/src/analysis/mod.rs` — `PreviewLanguageAdvisory` struct,
  `AnalysisResult::preview_language_advisories` field.
- `crates/ripr/src/analysis/pipeline.rs` — `is_preview_language()`,
  `build_diff_preview_advisory()`, population of `preview_advisories` in
  `run_diff_pipeline_with_oracle_policy`.
- `crates/ripr/src/app.rs` — `CheckOutput::preview_language_advisories` field.
- `crates/ripr/src/app/check/output_builder.rs` — maps advisory field through.
- `crates/ripr/src/output/human.rs` — `render_preview_language_advisories()`,
  `capitalize_first()`.
- `crates/ripr/src/output/json/report.rs` — additive `preview_languages`
  JSON block in `render_with_config`.
- `crates/ripr/src/output/diff_report.rs` — `DiffPreviewLanguageAdvisory`,
  `preview_languages` field on `DiffReport`.

## CI Proof

- `RUSTFLAGS="-D warnings" cargo build -p ripr -p xtask` — exit 0 each.
- `cargo test -p ripr` — all pass including 8 new disclosure tests.
- `cargo clippy -p ripr -p xtask --all-targets -- -D warnings` clean.
- `cargo fmt --check` clean.
- `cargo xtask check-static-language` pass.
- `cargo xtask check-architecture` pass.
- `cargo xtask check-no-panic-family` pass.
- `cargo xtask check-doc-artifacts` pass.
- `cargo xtask check-doc-index` pass.
- `cargo xtask check-spec-format` pass.
- `cargo xtask check-traceability` pass.
- `cargo xtask check-output-contracts` pass.
- Behavioral repro: `ripr check --root <ts-root> --diff <ts.diff>` shows the
  `Note:` line; `ripr check --diff crates/ripr/examples/sample/example.diff`
  shows NO preview note.

## Metrics

- Gate: all 8 acceptance tests pass.
- Promote to accepted when at least one external TypeScript repo exercises the
  disclosure path end-to-end and the empty-result honesty gap is confirmed closed.
