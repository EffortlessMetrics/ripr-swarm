# RIPR-SPEC-0112: Disclose an Unanalyzed Working Tree on Committed-History Runs

Status: accepted

Owner: product / swarm

Created: 2026-06-15

Linked issues:

- #1291 (check --base silently ignores uncommitted working-tree changes)
- #4008 (a bare `ripr check` excluded the working tree without disclosing it)

Linked PRs:

- [#1295](https://github.com/EffortlessMetrics/ripr-swarm/pull/1295) -
  implemented the `unanalyzed_working_tree` disclosure for committed-history
  `--base` runs with uncommitted tracked edits.

Support-tier impact:

- No tier change. This spec adds advisory disclosure output when `ripr check
  --base <rev>` is invoked and the working tree has uncommitted changes to
  tracked source files. It does not promote any feature to a higher support
  tier, does not change pass/fail authority, and does not alter what the
  analyzer classifies.
- The disclosure is additive output only. Claim boundaries remain governed by
  the canonical ledger in [support tiers](../status/SUPPORT_TIERS.md).
- Empty-result semantics remain unchanged: "No probes found" still means the
  static analyzer found no mutation exposure probes in the analyzed diff. The
  unanalyzed-working-tree case is additionally disclosed so an empty result
  cannot be read as "the uncommitted changes are covered."

Policy impact:

- Register this spec in `policy/doc-artifacts.toml`.
- No new crates, binaries, dependencies, parsers, runtime executors, or LSP
  servers introduced by this spec.

## Problem

When a user runs `ripr check --base <rev>`, the tool diffs committed history
(`<rev>..HEAD`) via `git diff`. Uncommitted working-tree changes to tracked
source files are silently excluded from the analyzed diff.

The critical false-clean case: a developer edits `src/lib.rs` without committing,
then runs `ripr check --base HEAD`. The committed diff is empty (`HEAD..HEAD`),
so the tool outputs:

```
No diff-derived static exposure probes found.
```

Exit code is 0. There are 0 findings. The result looks clean — but the developer's
actual uncommitted edit was never analyzed. This is the cardinal "silence reads as
clean" honesty failure for the `--base` mode.

This spec originally recorded that `ripr check` with no `--base` analyzed the
working tree, and scoped the disclosure to the explicit-flag path on that basis.
**That was wrong, and #4008 measured it.** RIPR-SPEC-0084 has `check` clear
`input.base` when no `--base` was typed, which sends the run through
`resolve_default_base` and produces a `<resolved>...HEAD` range: the same
committed-history diff, with the same working-tree exclusion. On identical
repository state a bare run and an explicit `--base` run return the same finding
ids and the same analyzed scope; only the second disclosed the exclusion.

The trigger below is therefore stated in terms of the analysis subject — a
committed-history range — rather than the flag that happened to select it.
`ripr check --worktree` (RIPR-SPEC-0116) is the mode that analyzes uncommitted
tracked edits.

## Behavior

### Trigger conditions

The disclosure fires when ALL of the following are true:

1. The CLI `check` command was invoked.
2. The run's analysis subject was a committed-history diff — the default
   findings path, whether the base was typed as `--base <rev>` or resolved by
   RIPR-SPEC-0084 — and a base was actually used (`output.base` is set).
3. `--diff <file>` was NOT provided (file-based diff is out-of-scope for
   working-tree disclosure: the subject is a file on disk, not a live range).
4. `--worktree` was NOT provided (RIPR-SPEC-0116 analyzes those edits, so there
   is nothing excluded to disclose).
5. The repo-scope formats were NOT selected (they read the tree from disk, so
   nothing is excluded).
6. The working tree has at least one uncommitted change to a tracked source
   file, as detected by `git status --porcelain` returning non-empty output.

The disclosure fires independent of whether `findings.is_empty()` — an
unanalyzed working tree is worth disclosing whether or not the committed diff
had findings. The false-clean risk is highest when findings are empty (the user
sees "no probes found" and infers clean), but the disclosure is honest in both
cases.

The guidance does NOT fire when:

- `ripr check --diff <file>` was given (file-based diff; not a live worktree).
- `ripr check --worktree` was given (uncommitted tracked edits are the subject).
- A repo-scope format was selected (the whole tree is read from disk).
- The worktree is clean (nothing uncommitted) — a clean result is honest.
- `git status --porcelain` cannot be run (fail-closed: no fabricated disclosure).

### Working-tree detection

`working_tree_has_tracked_changes(root: &Path) -> bool` runs
`git -C <root> status --porcelain` and returns `true` if stdout is non-empty.
The function lives in `crates/ripr/src/analysis/diff/load.rs`, alongside the
other git subprocess helpers (`run_git_diff`, `git_symbolic_ref_quiet`,
`git_ref_exists`). Fail-closed: if git cannot be run or returns a non-zero
exit code, the function returns `false` and no disclosure is fabricated.

### Human output

Every branch renders the same note, through one owner
(`output::human::render_unanalyzed_working_tree_note`):

```
Note: uncommitted changes to tracked source were not analyzed. `ripr check`
compares committed history only — run `ripr check --worktree` to include
uncommitted tracked edits, or commit them and re-run.
```

#4008 replaced two divergent earlier texts, each of which named a remedy that
does not work: staging does not move an edit into `<base>...HEAD`, and a bare
`ripr check` compares committed history exactly as an explicit `--base` does.

The note does not change the exit code or pass/fail status.

### JSON output (`--json`)

When `unanalyzed_working_tree` is true, an additive top-level field is emitted:

```json
"unanalyzed_working_tree": true
```

Absent when `unanalyzed_working_tree` is false. No schema version bump is
required per the additive field policy in [`docs/OUTPUT_SCHEMA.md`](../OUTPUT_SCHEMA.md).

### Non-claims

- This spec does NOT change the exit code or gate authority.
- An empty result with this disclosure does NOT mean the diff is safe; it
  means the committed history had no probes AND uncommitted changes were excluded.
- This spec does NOT change what the analyzer classifies.
- This spec does NOT analyze the uncommitted changes. `ripr check --worktree`
  (RIPR-SPEC-0116) is the mode that does.
- This spec does NOT change which runs analyze what. #4008 widened the
  disclosure to every committed-history run; it moved no analysis.

## Non-Goals

- Disclosure in SARIF, GitHub, badge, or repo-exposure output formats.
- Auto-staging or auto-analyzing uncommitted changes.
- Changing behavior when `--diff <file>` or `--worktree` is used.
- Runtime mutation testing, coverage measurement, or correctness claims.
- Changing what any run analyzes.

## Acceptance Examples

1. **The false-clean case (the bug)**: `ripr check --base HEAD` with an
   uncommitted `.rs` edit → human output includes the Note; JSON includes
   `"unanalyzed_working_tree": true`.
2. **Clean worktree with `--base`**: `ripr check --base HEAD` with NO
   uncommitted changes → no disclosure; result is genuinely honest.
3. **Committed diff with `--base`**: `ripr check --base HEAD~1` with committed
   changes (real findings) and a CLEAN worktree → no disclosure; result is
   honest.
4. **Bare run (#4008)**: `ripr check` with no flags and an uncommitted `.rs`
   edit → the same disclosure as case 1. The resolved default base makes this
   a committed-history run, so the exclusion is identical.
5. **File diff mode**: `ripr check --diff change.diff` → no disclosure; file
   diff is not a live worktree query.
6. **Worktree mode**: `ripr check --worktree` → no disclosure; uncommitted
   tracked edits are the analyzed subject.

## Required Evidence

- `CheckOutput.unanalyzed_working_tree: bool` field (additive, default `false`).
- `working_tree_has_tracked_changes(root)` git subprocess helper.

## Inputs

| Input | Required? | Purpose |
| --- | --- | --- |
| The selected analysis entry point | yes | Identifies a committed-history run, whatever flag selected it |
| `CheckOutput.base` | yes | Confirms a base was actually compared (#3940) |
| CLI flag `--diff` absence | yes | Ensures we are in live-worktree mode, not file mode |
| CLI flag `--worktree` absence | yes | Ensures uncommitted edits are not already the subject |
| `working_tree_has_tracked_changes(&root)` | yes | Detects uncommitted changes |

## Outputs

| Output | Schema impact | Notes |
| --- | --- | --- |
| Human text `Note:` line | None | Additive; absent when the worktree is clean or `--diff`/`--worktree`/a repo-scope format was used; does not change exit code |
| JSON `"unanalyzed_working_tree": true` | Additive field | Absent when false; no schema version bump |

## Test Mapping

- `crates/ripr/tests/cli_smoke.rs::check_base_head_with_uncommitted_edit_shows_unanalyzed_working_tree_disclosure`
- `crates/ripr/tests/cli_smoke.rs::check_base_head_with_clean_worktree_does_not_show_unanalyzed_working_tree_disclosure`
- `crates/ripr/tests/cli_smoke.rs::bare_check_with_uncommitted_edit_shows_unanalyzed_working_tree_disclosure` (#4008)
- `crates/ripr/tests/cli_smoke.rs::check_with_a_diff_file_does_not_show_unanalyzed_working_tree_disclosure` (#4008)

## Implementation Mapping

| Component | Location |
|---|---|
| `CheckOutput::unanalyzed_working_tree` field | `crates/ripr/src/app.rs` |
| `working_tree_has_tracked_changes` fn | `crates/ripr/src/analysis/diff/load.rs` |
| SPEC-0112 disclosure block | `crates/ripr/src/cli/commands/check.rs` |
| Human rendering | `crates/ripr/src/output/human.rs` (`render_unanalyzed_working_tree_note`) |
| JSON field | `crates/ripr/src/output/json/report.rs` |

## CI Proof

- `RUSTFLAGS="-D warnings" cargo build -p ripr -p xtask` — exit 0 each.
- `cargo test --workspace` — all pass including new smoke tests.
- `cargo clippy --workspace --all-targets -- -D warnings` clean.
- `cargo fmt --check` clean.
- `cargo xtask check-static-language` pass.
- `cargo xtask check-architecture` pass.
- `cargo xtask check-no-panic-family` pass.
- `cargo xtask check-doc-artifacts` pass.
- `cargo xtask check-doc-index` pass.
- `cargo xtask check-spec-format` pass.
- `cargo xtask check-traceability` pass.
- `cargo xtask check-output-contracts` pass.
- `cargo xtask check-support-tiers` pass.
- `cargo xtask check-process-policy` pass.
- Behavioral repro: (a) `ripr check --base HEAD` with uncommitted `.rs` edit
  prints the Note and JSON `unanalyzed_working_tree: true`; (b) `ripr check
  --base HEAD` with clean worktree shows NO disclosure; (c) #4008: a bare
  `ripr check` with an uncommitted `.rs` edit now prints the same disclosure,
  and `--diff` and `--worktree` still do not.

## Metrics

- Gate: both smoke tests pass.
- Promote to accepted when behavioral repro confirms the false-clean case is
  closed and the clean-worktree case shows no false disclosure.
