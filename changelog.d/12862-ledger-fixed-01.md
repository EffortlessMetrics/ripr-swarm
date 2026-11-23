<!-- section: Fixed -->
- TypeScript preview: in `ripr check --format json`, a finding whose repair
  packet is complete now carries `gap_state: actionable`,
  `actionability_category: complete_repair_packet`, `why_actionable` and
  `repair_action` lines in `evidence`, and its `missing` list no longer
  repeats the `advisory` / `incomplete_repair_packet` summary. Before, the
  JSON `evidence` and `missing` arrays could still describe a complete
  packet as blocked (#1358).
- Rust: a changed file reached by an absolute path (for example under an
  absolute `--root`) now takes its package from a `crates/<name>/src` or
  `crates/<name>/tests` segment anywhere in the path, so a test in the same
  package can be related to it (#1521).
- When `git status` cannot run or exits nonzero while ripr checks the
  working tree for uncommitted changes, ripr now prints a
  `ripr: working-tree change probe failed (...)` warning on stderr naming
  the failure. Before, the failure was silent and read the same as a clean
  tree, so the uncommitted-changes disclosure was dropped with no notice
  (#2165, #2074).
- The workflow from `ripr init --ci github` no longer lets a failed PR
  guidance report or SARIF render pass silently under a blocking
  `RIPR_GATE_MODE`: those steps now fail the job when the operator opted
  into blocking and stay advisory otherwise, while SARIF uploads remain
  advisory. A `Check RIPR advisory artifacts` step warns when expected
  artifacts, or the gate decision while `RIPR_GATE_MODE` is set, are
  missing (#2189, #2009).
- LSP: `workspace/didChangeWorkspaceFolders` now applies the event's added
  and removed folders to the server's stored folder set instead of
  re-asking the client and taking whatever list came back, so an answer that
  belongs to an earlier event no longer replaces the current root. An event
  with a non-file URI, a duplicate addition, a folder both added and
  removed, or the removal of an unknown folder is rejected with a named
  reason and leaves the folder set unchanged, and an event that leaves the
  same folder set no longer invalidates in-flight analysis (#2036, #2248).
- TypeScript checks: a test that mocks the changed function's own module
  (for example `jest.mock` on the owner's path) and asserts an exact value
  is no longer credited through a direct or imported owner call, since the
  call runs the mock. The finding now reads `weakly_exposed` with the
  `mocked_module` limitation instead of `exposed` (#2272, #2269).
- Windows: `ripr diff --json` now reports `changed_files[].path` and
  `changed_seams[].file` with portable `/` separators, instead of native
  backslashes and a leading `.\` (#2431, #2429).
- The VS Code extension VSIX no longer ships the compiled extension tests
  (`out/test/**`) or the test fixtures (`test-fixtures/**`) (#2492, #2439).
- The generated CI gate summary now shows the number of blocking findings
  and, when several block, the first reason followed by `(+N more, see
  gate-decision.md)`, instead of silently showing only the first blocking
  reason (#2748, #2622).
- VS Code: `ripr: Restart Server`, the restart after a configuration change
  and extension shutdown now share one lifecycle coordinator, so a restart
  that overlaps deactivation no longer starts the server again after
  shutdown, and a failed restart is logged and shown as an error message
  instead of failing silently (#2821).
- The Markdown gate report now names a missing placement instead of
  printing `<no path>:?`: a decision without a line reads
  `path (no line anchor)`, one without a file reads `(no file anchor):N`,
  and one with neither reads `(no changed-line anchor)`. The old
  `<no path>` text was also emitted as raw HTML. Decisions with a full
  placement render unchanged (#2855, #2641).
- Diff parsing no longer reads unresolved conflict-marker regions
  (`<<<<<<<` .. `>>>>>>>` on added or removed lines) as changed source, and
  no longer silently drops combined (`@@@`) merge hunks. Both are set aside
  as `unresolved_conflict_markers` and `combined_hunk_unsupported`
  limitations, and line numbers after the region stay correct (#2861,
  #2828).
- `ripr gate` now refuses a gap decision ledger, baseline or other gate
  input whose `analysis_outcome.analysis_complete` is `false`, naming the
  outcome kind, instead of gating on an incomplete denominator. The gap
  decision ledger carries the producer's `analysis_outcome` (and its
  Markdown states it), and the PR evidence summary JSON adds
  `analysis_complete` and `analysis_outcome`, reporting `run_status:
  incomplete` when analysis was not complete (#2881).
- LSP: an analysis whose typed outcome is incomplete or unsupported no
  longer reports `run_status` `full`. `ripr/analysisStatus`, workspace status
  and the top-limitation command now report `limited_incomplete_input` and
  carry the `analysis_outcome` with its limitation and recovery route (the
  top-limitation status reads `analysis_outcome_incomplete`), and a
  zero-finding snapshot with such an outcome emits no gap-record diagnostics
  and shows finding and seam Warnings as Information (#2882).
- `ripr pr-summary` JSON now reports `run_status: "unknown"` when no diff
  report was supplied, instead of borrowing the repo-exposure report's
  `run_status`, which could read `complete` for a diff that was never
  analyzed (#2907).
