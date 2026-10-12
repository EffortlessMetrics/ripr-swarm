# RIPR-SPEC-0116: `check --worktree` Mode

Status: accepted

Owner: product / swarm

Created: 2026-06-18

Linked issues:

- [#1296](https://github.com/EffortlessMetrics/ripr-swarm/issues/1296)
- [#3183](https://github.com/EffortlessMetrics/ripr-swarm/issues/3183) - LSP
  saved-workspace adoption of the canonical worktree diff authority.

Linked PRs:

- [#1325](https://github.com/EffortlessMetrics/ripr-swarm/pull/1325) -
  implemented explicit `ripr check --base <rev> --worktree` tracked-worktree
  diff mode and dirty `doctor` guidance.

Support-tier impact:

- No tier change. `docs/status/SUPPORT_TIERS.md` remains unchanged; this adds
  an explicit CLI input mode for draft analysis and does not change
  classifications, repair-packet authority, schema version, support tiers, or
  release claims.
- The #3183 amendment reuses that accepted input mode for LSP saved-workspace
  refreshes. It changes the editor's tracked-diff source, not its support tier,
  classification language, repair authority, or unsaved-buffer policy.
- Existing committed-history modes stay compatible. `--base <rev>` without
  `--worktree` still compares committed history and keeps the
  `unanalyzed_working_tree` disclosure from RIPR-SPEC-0112.

Policy impact:

- Register this spec in `policy/doc-artifacts.toml`.
- No new dependencies, crates, binaries, workflow permissions, or network/process
  surfaces beyond the existing local `git diff` adapter.

## Amendment (dirty-tree default and analyzed base/head header)

Owner decision (Steven): `ripr check` analyzes the working tree by default
when it has uncommitted changes, and every check output names the base and
head it analyzed.

- **Default diff source.** For a run that diffs the live repository (no
  `--diff`, no `--candidate-tree`, not a repo-scope format), the default reads
  the working tree exactly as `--worktree` does when
  `working_tree_has_uncommitted_changes` reports uncommitted work: a staged or
  unstaged edit to any tracked file. Untracked files never select the working
  tree, routed or not, because the working-tree diff (`git diff
  <merge-base>`) covers tracked files only. A clean or untracked-only tree,
  or a dirtiness probe that cannot run, keeps the committed-history read
  `git diff <base>...HEAD`. An unborn or dangling `HEAD` also keeps it,
  because staged files there read as additions and would otherwise bypass the
  committed-history refusal; that run's own loader names any git failure, so
  the probe adds no second warning, and on an untracked-only tree the
  RIPR-SPEC-0112 note names the untracked files and the staging repair
  (#5258).
- **Explicit `--base` follows the same default.** The base names where the
  diff starts, not where it ends, so `ripr check --base origin/main` on a
  dirty tree reads the working tree. CI checkouts are clean, so CI runs keep
  the committed-history read unchanged.
- **`--committed`** forces the committed-history read on a dirty tree. It is
  rejected with `--worktree`, `--diff`, `--candidate-tree`, `--gap-ledger`,
  and repo-scope formats, none of which has a live diff source to choose.
- **One owner.** `app::diff_source::select_live_diff_source` owns the
  decision so `ripr pilot` can reuse it; the CLI adapter only decides whether
  a live diff source exists.
- **Drill-in parity.** A working-tree default run carries `--worktree` into
  its printed `explain`/`context` commands and records the worktree diff
  source in `--write-artifact`, exactly as an explicit `--worktree` run does.
- **Untracked files.** Untracked files are not in the working-tree diff until
  staged or marked intent-to-add, so they never flip the default (above). A
  working-tree read, default-selected or `--worktree`, discloses the
  untracked files a language adapter routes: the human output prints a note
  and the GitHub stream a `ripr untracked files not analyzed` warning that
  name them (at most three, then a count), say they are not in the
  working-tree diff, and give the repair (`git add -N <path>` or stage them).
  The note never suggests `--worktree`, which is already in effect. An
  untracked test file is still read as test evidence on a working-tree run.
- **Empty working-tree read.** When a working-tree read finds no changed
  files, the no-scope note, safe next action and GitHub warning describe the
  working-tree read (the merge base of the base and `HEAD` to the working
  tree), not `<base>...HEAD`, and do not suggest `--worktree`.
- **Analyzed base and head.** Every diff-scoped live-repository run names its
  base ref and commit and its head, either `HEAD <sha>` or
  `working tree (uncommitted changes on HEAD <sha>)`; when the merge base
  the diff started from differs from the base tip, the human and GitHub
  labels also name it. Human and human-full print `base:`/`head:` header
  lines; JSON adds `base_commit`, optional `merge_base_commit`, and
  `head {source, commit}` beside the existing `base`; GitHub leads with a
  `ripr analyzed` notice; SARIF carries the same fields in run `properties`.
  A `--candidate-tree` human header names the base tree and candidate tree.
  `--diff` file and stdin runs carry no revisions ripr can verify, so their
  output is unchanged; badge and repo-scope formats have no diff header.
- **LSP and library callers are unchanged.** The LSP already reads the
  working tree for every refresh (`check_workspace_worktree_*`), and the
  public `check_workspace*` library entry points keep their committed-history
  read; neither routes through the CLI default.

This supersedes the original non-goal "Changing default `ripr check`
semantics" and the compatibility rule that `--base <rev>` without
`--worktree` always reads committed history.

## Problem

RIPR is a draft-time evidence tool, but the obvious first-run path still has an
awkward gap for uncommitted work. `ripr check --base HEAD` compares committed
history and therefore excludes staged/unstaged tracked edits. RIPR-SPEC-0112
made that exclusion visible, but it still tells a new user to commit or stage
before RIPR can analyze their actual draft.

The desired first-run behavior is explicit and honest:

```text
ripr check --base HEAD --worktree
```

That command should analyze the live tracked working tree against `HEAD`, so an
empty result means the working tree really has no tracked diff against the base.

## Behavior

### CLI contract

`ripr check` accepts a new `--worktree` flag.

When `--worktree` is present:

- the diff source is `git diff <base>` instead of `git diff <base>...HEAD`;
- staged and unstaged tracked edits are included;
- committed changes since `<base>` are also included;
- untracked files remain out of scope until staged or supplied through
  `--diff`;
- `--diff <file>` is rejected because a file diff and a live worktree diff are
  mutually exclusive scope sources;
- `--worktree` counts as an explicit analysis scope, so no-scope disclosure does
  not fire.
- the printed drill-in commands (`ripr explain`, `ripr context`) carry
  `--worktree` after `--base`, so they read the same uncommitted scope as the
  check that printed them; `ripr explain` and `ripr context` accept
  `--worktree` and reject it combined with `--diff` or `--from`.

When `--worktree` is absent:

- a live-repository run (with or without `--base`) reads the working tree
  when it has uncommitted changes and committed history when it is clean (see
  the amendment above);
- `--committed` forces committed-history mode, which may emit
  `unanalyzed_working_tree`;
- `--diff <file>` remains file-based mode;
- default-base resolution is unchanged.

In every mode, the printed drill-in commands (`ripr explain`, `ripr context`,
the `ripr check` listing, `ripr agent stub`, and the `ripr context --json`
`witness.explain_command`) name the repository `check` resolved as an absolute
`--root`, and a relative `--diff`, `--from` or `--perl-facts` as an absolute
path (the stdin sentinel `-` stays), never the relative spelling repeated as
typed, so pasting one from another directory analyzes the same repository
(#3948).

### Doctor guidance

When `ripr doctor --root <repo>` sees staged or unstaged tracked changes **and
git is available**, it recommends:

```text
ripr check --base HEAD --worktree
```

and names the boundary that untracked files remain out of scope until staged or
provided via `--diff`.

When git is not on PATH, both `ripr check` and `--worktree` fail the same way.
That includes the zero-config path (`ripr check` with no `--base`): default-base
search must not diagnose missing git as an unresolvable ref. Doctor's `!` line
names the same repair as `ripr check` in that environment (install git, or pass
a saved diff with `--diff PATH` / `--diff -`), and the recommended first
command is `ripr check --diff PATH`. A dirty worktree does not override that:
`--worktree` cannot run without git.

`ripr check` defaults to `--root .`, so when doctor diagnosed a root other than
`.` each recommended command names it, bound to an absolute path against
doctor's directory and shell-quoted: `ripr doctor --root
/work/app` recommends `ripr check --root /work/app --base HEAD --worktree`,
`ripr check --root /work/app`, or `ripr check --root /work/app --diff PATH`.
When PowerShell needs a different form (a root containing an apostrophe), a
labeled `(PowerShell)` line follows (#4890).

### LSP saved-workspace contract

The LSP refresh path consumes the same tracked-worktree diff source as
`check --worktree`. Both interactive save refreshes and explicit full refreshes
therefore include staged and unstaged tracked edits without requiring a commit.
The refresh scope still controls only the seam inventory: interactive refreshes
defer it and disclose `seams_deferred`, while explicit refreshes run it.

Document lifecycle and diff scope remain separate authorities. Unsaved buffers
stay quarantined and never enter analysis. Untracked files remain outside the
worktree diff until staged or supplied through an explicit diff, and the LSP
workspace-status limits note names that boundary so zero findings cannot imply
that untracked source was analyzed.

## Non-Goals

- Auto-staging, reading untracked files, or inventing a diff for untracked
  files.
- Changing default `ripr check` semantics (superseded by the amendment above).
- Changing `--diff` file semantics.
- Adding or renaming output fields.
- Promoting any finding based only on worktree scope.
- Pinning a drill-in to the edits that existed when `check` ran. A worktree
  drill-in re-reads the working tree when it runs, so an edit made between
  `check` and `explain`/`context` can move a `file:line` or change a finding
  id; the miss message then names the same-scope listing to re-list ids.

## Acceptance Examples

1. **Dirty tracked worktree**: a tracked Rust source file is edited after
   `HEAD`. `ripr check --base HEAD --worktree --json` emits findings for the
   changed source and does not emit `unanalyzed_working_tree`.
2. **Clean worktree**: `ripr check --base HEAD --worktree --json` emits no
   findings and no scope/unanalyzed-worktree disclosure.
3. **Committed-history compatibility**: `ripr check --base HEAD --committed
   --json` with a dirty tracked source or test file still emits
   `unanalyzed_working_tree: true` and `head.source: "commit"`.
4. **File diff compatibility**: `ripr check --diff change.patch` keeps existing
   behavior; `ripr check --diff change.patch --worktree` returns an error.
5. **Drill-in parity**: after `ripr check --base HEAD --worktree` finds an
   uncommitted change, its printed `ripr explain` and `ripr context` commands
   carry `--worktree` and, run verbatim from another directory, select that
   finding; the same explain without `--worktree` finds nothing.
   `ripr context --worktree --json` names an explain command that carries
   `--worktree`. A selector miss or a missing selector under `--worktree`
   names a `ripr check ... --worktree --json` listing with the same root and
   base, and a `--worktree` drill-in without `--root` resolves the project
   root from a subdirectory the way `ripr check` does.
6. **Doctor**: dirty tracked-worktree guidance names
   `ripr check --base HEAD --worktree` when git is available. When git is not
   on PATH, doctor names the `--diff` route even if the tree is dirty.
7. **LSP saved edit**: with an empty `HEAD...HEAD` committed diff and a tracked
   saved source edit in `git diff HEAD`, an interactive LSP diagnostic refresh
   emits diff-scoped findings while keeping the seam inventory deferred.
8. **LSP explicit refresh parity**: an explicit full refresh consumes the same
   worktree diff and differs only by running the full seam inventory.
9. **Dirty-tree default**: bare `ripr check` with an uncommitted tracked edit
   lists the same findings as `ripr check --worktree`, emits no
   `unanalyzed_working_tree`, names `head: working tree (uncommitted changes
   on HEAD <sha>)` in human output and `head.source: "working_tree"` in JSON,
   and prints drill-ins that carry `--worktree`. An untracked file alone, even
   a routed source or test file, does not select the working tree: the run
   reads committed history and prints the RIPR-SPEC-0112 note naming it. A
   working-tree read beside an untracked routed file names that file and the
   intent-to-add repair, with no `--worktree` advice; an empty working-tree
   read describes the merge-base-to-working-tree diff.
10. **Clean-tree default**: bare `ripr check` on a clean tree (an untracked
   non-source file does not count) reads committed history and names
   `head: HEAD <sha>`.

## Required Evidence

- The CLI carries `--worktree` as an explicit internal analysis mode without
  changing the public `CheckInput` or `AnalysisOptions` struct shape.
- `analysis::diff::load_worktree_diff` uses `git diff <base>` and default-base
  resolution when no explicit base is supplied.
- CLI parser accepts `--worktree`, rejects `--diff` plus `--worktree`, and keeps
  existing `--base` / `--diff` behavior unchanged.
- Doctor dirty tracked-worktree guidance recommends the worktree command when
  git is available; untracked-only files do not trigger the tracked-edit
  recommendation. When git is not on PATH, the `!` line and recommended first
  command name the `--diff` route instead.
- CLI smoke tests cover dirty worktree, clean worktree, and doctor guidance.
- LSP interactive and explicit refreshes share the worktree diff producer;
  refresh scope continues to decide only whether seam inventory is deferred.
- LSP workspace status names the untracked-file boundary, while document
  quarantine continues to exclude unsaved buffers from current evidence.

## Test Mapping

- `crates/ripr/tests/cli_smoke.rs::check_worktree_base_head_analyzes_uncommitted_tracked_edit`
- `crates/ripr/tests/cli_smoke.rs::check_worktree_drill_in_commands_reach_the_uncommitted_finding`
- `crates/ripr/src/app/navigation.rs::tests::finding_navigation_carries_worktree_scope_after_the_base`
  - dirty tracked edit produces findings and no unanalyzed-worktree disclosure.
- `crates/ripr/src/app/navigation.rs::tests::finding_navigation_binds_a_relative_root_for_every_drill_in`
- `crates/ripr/src/app/navigation.rs::tests::finding_navigation_binds_relative_input_files_and_keeps_the_stdin_sentinel`
  - drill-ins name the resolved root and bound input files (#3948).
- `crates/ripr/tests/cli_smoke.rs::check_worktree_base_head_clean_worktree_has_no_scope_or_unanalyzed_disclosure`
  - clean worktree produces no findings and no scope/unanalyzed-worktree
  disclosure.
- `crates/ripr/tests/cli_smoke.rs::doctor_recommends_worktree_check_on_dirty_worktree`
  - dirty doctor guidance names the new command.
- `crates/ripr/tests/cli_smoke.rs::doctor_without_git_names_the_fix_and_recommends_the_diff_route`
  - a gitless doctor `!` line names install git and `--diff`, and recommends
    `ripr check --diff PATH`.
- `crates/ripr/tests/cli_smoke.rs::doctor_without_git_does_not_recommend_worktree_on_a_dirty_tree`
  - a dirty tree cannot win over a missing git binary.
- `crates/ripr/tests/cli_smoke.rs::check_without_git_names_path_and_diff_routes_without_dumping_argv`
  - `check` names PATH and `--diff` instead of dumping git argv; `--diff` still
    runs without git.
- `crates/ripr/tests/cli_smoke.rs::check_without_git_omitted_base_names_path_not_unresolvable_base`
  - a gitless `check` with no `--base` names PATH, not `Pass --base`.
- `crates/ripr/src/analysis/diff/load.rs::tests::git_root_probe_prefers_missing_git_over_unresolved_base`
  - the git-root probe maps a missing-git spawn to PATH, not default-base text.
- `crates/ripr/src/analysis/diff/load.rs::tests::git_root_probe_names_a_non_repo_after_git_ran`
  - a git that ran outside a work tree keeps the non-repo diagnosis.
- `crates/ripr/src/analysis/diff/load.rs::tests::git_root_probe_does_not_invent_a_cause_when_git_ran_inside_a_work_tree`
- `crates/ripr/src/analysis/diff/load.rs::tests::git_root_probe_does_not_invent_a_cause_on_timeout`
- `crates/ripr/src/cli/commands.rs::tests::check_rejects_diff_file_plus_worktree_mode`
  - `--diff` and `--worktree` remain mutually exclusive.
- `crates/ripr/src/analysis/diff/load.rs::tests::tracked_change_detector_ignores_untracked_only_files`
  - untracked-only files do not trigger tracked-worktree guidance.
- `crates/ripr/src/analysis/diff/load.rs::tests::tracked_change_detector_detects_tracked_edit`
  - staged or unstaged tracked edits still trigger tracked-worktree guidance.
- `crates/ripr/src/analysis/diff/load.rs::tests::tracked_change_detector_ignores_parent_repo_changes_outside_root`
  - parent-repo tracked edits outside the requested root do not trigger
    tracked-worktree guidance for nested roots.
- Existing RIPR-SPEC-0112 tests continue to prove committed-history compatibility.
- `crates/ripr/src/lsp/tests.rs::lsp_saved_worktree_refresh_analyzes_uncommitted_tracked_edit`
  - proves the committed diff is empty, the tracked worktree diff contains the
    saved source file, interactive seams remain deferred, and the real LSP
    diagnostic producer emits the source diagnostic.
- `crates/ripr/src/lsp/tests.rs::framed_code_lens_refresh_follows_semantic_lens_view_changes`
  - proves the framed explicit-refresh consumer retains a worktree-derived
    semantic lens view across a repeated full refresh.
- `crates/ripr/src/lsp/tests.rs::workspace_diagnostics_include_saved_tracked_edits_and_exclude_untracked_files`
  - proves deferred LSP analysis emits diff-scoped findings for both unstaged
    and staged tracked edits without passing through full seam inventory, and
    proves a separate untracked-only workspace emits no findings or diagnostic
    batch for that source.
- `crates/ripr/src/lsp/tests.rs::framed_lsp_saved_workspace_session_serves_saved_state_across_dirty_save`
  - proves the real framed `didSave` path publishes the RIPR diagnostic on the
    changed saved source line before the later explicit full refresh, while the
    dirty pre-save document remains quarantined.

- `crates/ripr/tests/cli_smoke.rs::check_default_base_with_uncommitted_edit_analyzes_the_working_tree`
  - dirty default equals `--worktree`, names base/head in human and JSON, and
    `--committed` restores the committed read with its disclosure.
- `crates/ripr/tests/cli_smoke.rs::check_default_base_with_clean_worktree_keeps_no_scope_note_only`
  - clean default names `head: HEAD <sha>` and `head.source: "commit"`.
- `crates/ripr/tests/cli_smoke.rs::check_base_reads_tests_as_committed_and_notes_only_source_changes`
  - with an explicit `--base`, an edited test moves the default result while
    `--committed` keeps the committed result and notes it; an untracked test
    alone keeps the default on committed history with the same note.
- `crates/ripr/tests/cli_smoke.rs::check_untracked_files_keep_committed_default_and_working_tree_reads_name_them`
  - an untracked-only tree keeps the committed default and the #5258 note; a
    tracked edit beside untracked `src/new.rs` reads the working tree and
    names new.rs in human and GitHub output without `--worktree` advice; an
    empty default working-tree read (a staged edit reverted in the working
    tree) describes the working-tree diff in human and JSON output, not
    `main...HEAD`.
- `crates/ripr/src/output/human.rs::tests::empty_working_tree_read_describes_the_working_tree_and_names_untracked_files`
- `crates/ripr/src/output/human.rs::tests::candidate_tree_header_names_base_and_candidate_trees`
- `crates/ripr/src/output/github.rs::tests::render_leads_with_analyzed_base_and_head_notice`
- `crates/ripr/src/output/github.rs::tests::working_tree_read_warns_on_untracked_files_and_describes_its_range`
- `crates/ripr/src/output/sarif.rs::tests::sarif_run_properties_name_analyzed_base_and_head`
- `crates/ripr/src/app/analysis_outcome_artifact.rs::tests::validates_working_tree_head_source_against_the_working_tree_diff`
- `crates/ripr/tests/cli_smoke.rs::check_committed_rejects_conflicting_diff_sources`
- `crates/ripr/src/app/diff_source.rs::tests::default_reads_the_working_tree_only_when_it_is_dirty`
- `crates/ripr/src/app/diff_source.rs::tests::forced_sources_win_without_running_the_probe`
- `crates/ripr/src/analysis/diff/load.rs::tests::uncommitted_change_detector_counts_tracked_edits_not_untracked_files`
- `crates/ripr/src/output/analyzed_revisions.rs::tests::labels_name_ref_short_commits_and_the_head_source`
- `crates/ripr/src/output/analyzed_revisions.rs::tests::labels_disclose_a_distinct_merge_base_and_unresolved_commits`

## Implementation Mapping

| Component | Location |
|---|---|
| Default diff-source selection | `crates/ripr/src/app/diff_source.rs` |
| Dirtiness probe and revision resolution | `crates/ripr/src/analysis/diff/load.rs` |
| Base/head labels | `crates/ripr/src/output/analyzed_revisions.rs` |
| Base/head in human, JSON, GitHub, SARIF | `crates/ripr/src/output/human.rs`, `crates/ripr/src/output/json/report.rs`, `crates/ripr/src/output/github.rs`, `crates/ripr/src/output/sarif.rs` |
| CLI flag parse and doctor guidance | `crates/ripr/src/cli/commands.rs` |
| Doctor first-command owner | `crates/ripr/src/cli/commands/doctor.rs` |
| Doctor git-unavailable first command | `crates/ripr/src/output/doctor.rs` |
| Git spawn missing-PATH diagnosis | `crates/ripr/src/git.rs` |
| User help | `crates/ripr/src/cli/help/core.rs` |
| Drill-in and listing commands | `crates/ripr/src/app/navigation.rs` |
| Worktree explain/context use cases | `crates/ripr/src/app/explain.rs`, `crates/ripr/src/app/context.rs` |
| Worktree explain/context CLI adapters | `crates/ripr/src/cli/commands.rs`, `crates/ripr/src/cli/commands/context.rs` |
| Implicit root resolution | `crates/ripr/src/cli/commands/check.rs` |
| App-internal worktree check path | `crates/ripr/src/app/check.rs` |
| Analysis worktree pipeline | `crates/ripr/src/analysis/mod.rs` |
| Diff source selection | `crates/ripr/src/analysis/pipeline.rs` |
| Worktree diff loader | `crates/ripr/src/analysis/diff/load.rs` |
| LSP saved-workspace consumer | `crates/ripr/src/lsp/diagnostics.rs` |
| LSP untracked-scope disclosure | `crates/ripr/src/lsp/backend.rs` |

## CI Proof

- `cargo test -p ripr --test cli_smoke worktree`
- `cargo test -p ripr --test cli_smoke doctor_recommends_worktree_check_on_dirty_worktree`
- `cargo test -p ripr --test cli_smoke doctor_without_git`
- `cargo test -p ripr --test cli_smoke check_without_git_names_path_and_diff_routes_without_dumping_argv`
- `cargo test -p ripr --test cli_smoke check_without_git_omitted_base_names_path_not_unresolvable_base`
- `cargo test -p ripr --lib check_rejects_diff_file_plus_worktree_mode`
- `cargo test -p ripr --lib tracked_change_detector`
- `cargo test -p ripr --lib lsp::tests::lsp_saved_worktree_refresh_analyzes_uncommitted_tracked_edit -- --exact`
- `cargo test -p ripr --lib lsp::tests::framed_code_lens_refresh_follows_semantic_lens_view_changes -- --exact`
- `cargo test -p ripr --lib lsp::tests::workspace_diagnostics_include_saved_tracked_edits_and_exclude_untracked_files -- --exact --nocapture --test-threads=1`
- `cargo test -p ripr --lib lsp::tests::framed_lsp_saved_workspace_session_serves_saved_state_across_dirty_save -- --exact --nocapture --test-threads=1`
- Each fully qualified exact LSP selector above must report `running 1 test`;
  a zero-test success is not acceptance evidence.
- `cargo test -p ripr`
- `cargo fmt --check`
- `cargo check --workspace --all-targets`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `cargo xtask check-spec-format`
- `cargo xtask check-doc-artifacts`
- `cargo xtask check-static-language`
- `cargo xtask check-traceability`
- `cargo xtask check-output-contracts`

## Metrics

- Gate: worktree-mode CLI smoke tests pass.
- Gate: tracked-change detector unit tests pass, including untracked-only false
  recommendation protection and parent-repo dirty-state isolation.
- Gate: the LSP saved-worktree fixture has an empty committed diff, a nonempty
  tracked worktree diff, deferred seams, and a nonempty source diagnostic.
- Promote to accepted when the dirty tracked edit, clean worktree,
  committed-history compatibility, and `--diff`/`--worktree` rejection examples
  all pass in the PR proof set.
