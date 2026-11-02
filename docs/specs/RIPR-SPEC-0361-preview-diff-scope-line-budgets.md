# RIPR-SPEC-0361: Preview-language diffs get the same changed-line budget refusal as Rust

Status: proposed

Owner: product / analysis

Created: 2026-10-26

Linked proposal:

- None yet

Linked ADRs:

- None yet

Linked plan:

- None yet

Linked issues:

- #10730

Linked PRs:

- #10803

Support-tier impact:

- No tier change. This adds an analysis-cost guard; it adds no class, oracle
  kind or relation reason.
- Claim boundaries remain governed by
  [support tiers](../status/SUPPORT_TIERS.md).

Policy impact:

- Register this spec in `.ripr/traceability.d/` per the fragment convention.
- No schema version bump: `analysis_scope.basis` is an open string and gains
  `python_diff_scope_budget` / `typescript_diff_scope_budget` alongside the
  existing `rust_diff_scope_budget`.

## Problem

Only Rust diffs had a changed-line budget: over
`RIPR_MAX_DIFF_CHANGED_RUST_LINES` (default 2000 added plus removed lines) the
run fails closed with a typed `diff_scope_oversized` refusal before probe
expansion. Python and TypeScript diffs had no equivalent, so a 167-file django
diff took 212 s, a 221-file diff hit the 600 s timeout with zero findings, and
large-repo TypeScript diffs burned minutes in workspace discovery — with no
explainable refusal to repair from.

## Behavior

1. `ripr check` measures the added plus removed changed lines of the files
   each enabled preview adapter would actually admit — Python under
   `RIPR_MAX_DIFF_CHANGED_PYTHON_LINES`, and the whole routed TypeScript /
   JavaScript family (`.ts`/`.tsx`/`.mts`/`.cts`/`.js`/`.jsx`/`.mjs`/`.cjs`)
   under `RIPR_MAX_DIFF_CHANGED_TYPESCRIPT_LINES` — each defaulting to 2000
   and overridable exactly like the Rust limit (a positive integer; invalid
   values fail closed naming the variable; there is no `0` disables escape).
2. The count excludes files the adapter's own authorities refuse before
   counting — detectable generated names and excluded directories, minus
   `handwritten_files` declarations — the same admission predicate the
   pipeline uses for preview analysis. Generated bulk therefore cannot refuse
   a diff whose admitted hand-written surface is under the limit; this
   mirrors the Rust guard, which counts the diff minus its generated sources.
3. The check runs at the pipeline dispatch level after the Rust adapter and
   before any preview adapter, so a refusal propagates exactly like the Rust
   guard's. The refusal text starts with the `diff_scope_oversized:` prefix,
   names the admitted line and file counts, the limit, and its env override,
   and names the `--path`/`--exclude` repair route, in the Rust guard's voice.
   The prefix keeps `is_diff_scope_oversized`, the LSP limited-snapshot
   conversion, and the CLI exit-2 path working unchanged.
4. Only enabled adapters are guarded: a Python-heavy diff under the default
   Rust-only language set is never refused by a guard for an adapter that
   will not run.
5. In the `--json` limited artifact, `analysis_scope.basis` (and
   `run_limitations[].basis`) name the budget that fired:
   `python_diff_scope_budget`, `typescript_diff_scope_budget`, or
   `rust_diff_scope_budget` for the pre-existing Rust guards — a field never
   claims more than it enforces.

## Required Evidence

- Over-limit admitted Python and TypeScript diffs are refused before the
  adapter runs, with the prefix, counts, env var and repair route in the
  message.
- At-limit diffs pass; invalid or non-positive env values fail closed naming
  the variable; unset yields the 2000 default.
- Generated-name and excluded-directory lines do not count toward either
  budget: 2001 generated lines plus one admitted line pass.
- A generated name stays excluded even when declared `handwritten_files`; a
  declared path under an excluded directory is admitted and counted.
- The `--json` limited artifact carries `python_diff_scope_budget` /
  `typescript_diff_scope_budget` for the new refusals and keeps
  `rust_diff_scope_budget` for both Rust refusals.
- A Python-only oversized diff under the default Rust-only language set is
  not refused.

## Non-Goals

- Per-language changed-FILE guards: the Rust file guard
  (`RIPR_MAX_DIFF_INDEX_FILES`) is diff-wide, so there is nothing per-language
  to mirror.
- Warm-path / caching work (#10725) and vscode workspace-discovery
  performance. The `diff/load.rs` alternate-base probe's preview-budget
  screening is #10821, not this guard slice.
- MCP/LSP protocol changes: the existing `diff_scope_oversized` prefix
  handling is the whole integration surface.
- Default timeout changes.

## Acceptance Examples

1. `ripr check --diff django.diff` with 2100 admitted changed Python lines
   and the Python adapter enabled: exits non-zero with
   `diff_scope_oversized: 2100 changed Python lines ...` naming
   `RIPR_MAX_DIFF_CHANGED_PYTHON_LINES`, `--path` and `--exclude`; `--json`
   emits the limited artifact with `basis: "python_diff_scope_budget"`.
2. The same diff where 2001 of the lines are in `pkg/generated_models.py`:
   the run proceeds; the generated file's lines do not count.
3. `RIPR_MAX_DIFF_CHANGED_TYPESCRIPT_LINES=5000 ripr check --diff web.diff`:
   a 4500-line TypeScript diff is analyzed instead of refused.

## Test Mapping

- `crates/ripr/src/analysis/pipeline.rs` tests: the
  `diff_changed_{python,typescript}_line_limit_*` env-parsing tests,
  `preview_line_limits_resolve_forced_env_without_process_mutation`,
  `changed_{python,typescript}_line_count_*` counting and admission tests,
  `changed_{python,typescript}_line_limit_{rejects_oversized_diff_before_probe_expansion,accepts_at_limit}`,
  `preview_line_limits_pass_an_at_limit_diff`,
  `oversized_{python,typescript}_diff_is_refused_before_{python,typescript}_analysis`,
  `oversized_python_diff_passes_when_python_is_not_enabled`,
  `generated_bulk_does_not_refuse_a_small_admitted_{python,typescript}_diff`.
- `crates/ripr/src/output/limited_check.rs` tests:
  `python_budget_refusal_names_python_diff_scope_budget`,
  `typescript_budget_refusal_names_typescript_diff_scope_budget`,
  `rust_budget_refusals_keep_the_rust_basis`.

## Implementation Mapping

- `crates/ripr/src/analysis/pipeline.rs`: the two limits and env resolvers,
  `changed_language_line_count` (admission-filtered),
  `language_admits_changed_path`, `enforce_changed_language_line_limit`,
  `enforce_changed_preview_line_limits`, and the dispatch-level call site.
- `crates/ripr/src/output/limited_check.rs`: `diff_scope_basis` selects the
  typed basis for the `--json` limited artifact.
- `crates/ripr/src/analysis/language/rust/mod.rs`: `diff_limit_env`,
  `positive_limit_from_env`, `with_forced_diff_limit_env` widened to
  `pub(crate)` so the preview guards reuse the shared env machinery.

## Metrics

- `unit_test_pass_rate`: the tests above are required Rust tests.
