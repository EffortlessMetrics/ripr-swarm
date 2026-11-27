# RIPR-SPEC-0318: Pilot next steps name the dirty-target precondition and the continue command

Status: proposed

Owner: product / output

Created: 2026-10-22

Linked proposal:

- None yet

Linked ADRs:

- None yet

Linked plan:

- None yet

Linked issues:

- #3896 (the documented repair step needs a command the user can run)
- #9553 (an index-only difference needs the same precondition)
- #10037 (share the target observation across human summaries)
- #16381 (a dirty production premise makes the printed start refuse)

Linked PRs:

- #8520

Support-tier impact:

- No tier change. The pilot's next-step text is human copy/paste guidance, not
  a new oracle kind, strength, class, relation reason or limitation kind.
- Claim boundaries remain governed by
  [support tiers](../status/SUPPORT_TIERS.md).

Policy impact:

- Register this spec in `.ripr/traceability.toml`.
- No schema version bump. The pilot summary JSON is unchanged; only terminal
  and Markdown next-step prose changes.

## Problem

`ripr pilot` printed `Next, in order: 1. ripr agent repair ... --phase before`
for a repository with uncommitted edits to a production file. When the repair
targets an inline `#[cfg(test)]` module in that file, the repair start refuses
it (`require_inline_target_at_head`: the target must match HEAD), after pilot
had already written its workflow files. The last step also told the user to
run an `--attempt ... --phase after` command, while step 1 prints
`ripr continue --attempt ...`.

## Behavior

### Rule 1: dirty inline-target precondition

When the top seam's repair targets an inline test module and that target
file differs from HEAD in the index or the worktree, the terminal next steps
start with one line before step 1: if the target file has uncommitted edits,
commit or stash them, because the repair start refuses a target that differs
from HEAD. The decision reads the target file's own state through the edit
cage's `inline_target_is_uncommitted` (the predicate the repair start's
baseline capture agrees with), not pilot's worktree-diff change state, so a
staged-only difference (index differs, worktree matches the index, pilot sees
no change) prints the line too. A target at HEAD, and a `tests/` target, print
no such line, because the refusal does not apply to them. The predicate fails
open (no line) whenever the answer is not positively established, leaving the
repair start's own refusal authoritative.
In particular, an unborn HEAD or a root below the Git top-level prints no
precondition line: the repair start refuses those roots itself. A clean target
also prints no line when another file is dirty and the current change carries
`working_tree=true`.

The pilot command observes the inline target once and shares the answer between
the terminal and saved Markdown summary. Rendering both surfaces must not repeat
the Git probes; their wording remains unchanged.

### Rule 2: continue command

The terminal's final step and the Markdown summary's repair introduction name
the `continue --attempt ...` command that step 1 prints, not an `--attempt ...
--phase after` spelling that step 1 never prints.

### Rule 3: dirty production premise names the commit first

When the top seam offers a repair start (the existing `repair_start_command`
gate; a run that prints no start spawns no probe), the pilot renderer asks the
before phase's own policy-free dirty-premise preflight once, through
`app::seam_start_premise` (which calls
`app::repair_attempt::seam_start_dirty_premise`, the check the start applies
before it refuses with "ripr repair needs your source changes committed first
... Nothing was started"), in `ripr check`'s order, and shares the answer
between both human surfaces. The preflight compares tracked files with HEAD as
`git diff HEAD` does: a staged or unstaged edit is dirty, while an index-only
difference whose worktree copy matches HEAD is not. It reports production
files only, relative to the analyzed root (`../...` outside it).

1. **Unreadable premise.** No start is shown. Both surfaces print `ripr
   check`'s advisory for the same state: `no repair start offered: could not
   validate the prerequisites for the repair start; inspect the current
   workspace and rerun `ripr check``.
2. **Dirty premise under a subdirectory root** (check's
   `enclosing_git_top_level_above` gate). The start refuses ("attempt
   root ... is not the exact Git top-level") whatever is committed, so no
   commit step and no start are shown. Both surfaces print `ripr check`'s
   subdirectory advisory: `no repair start offered: this root is not the
   exact Git top-level (<top>), and a repair attempt starts only there; ...`.
   The terminal prints it as `repair this gap: <advisory>` and as the only
   line under `Next:`.
3. **Dirty premise, every file inside the root.** The terminal prints, ahead
   of `repair this gap:`,

   ```text
   commit your production change first (the repair start refuses to begin while it is uncommitted): git -C <root> --literal-pathspecs commit -m "<message>" -- <files>
   ```

   and, between `Next, in order:` and step 1,

   ```text
   Before step 1, commit your production change (the repair start refuses to begin while it is uncommitted):
      git -C <root> --literal-pathspecs commit -m "<message>" -- <files>
   ```

   The Markdown summary introduces its next-command block with "Commit your
   production change first (...)" and lists the commit before the start. The
   command is the shared `commit_prerequisite_command` spelling `ripr check`
   prints for the same state (#13636). Its files are every file the preflight
   reports, followed by a held inline target that the preflight skipped, each
   named once. It replaces the Rule 1 line when both apply.
4. **Dirty premise naming a file outside the root.** Such a path (`../...`)
   exists only below the Git top-level, where the attempt refuses. This
   includes a nested Cargo project the enclosing package does not claim:
   check's top-level gate keeps that root (#15488), but the attempt does not.
   Pilot prints no copyable command naming a path outside the analyzed root,
   and no in-root-only commit as if it cleared the premise. It withholds the
   start with check's subdirectory advisory, naming the exact Git top-level,
   as in case 2.

A committed change, and a dirty test-surface file (the refusal's own
allowance), add no step. Pilot reads the working tree it ranks, so every file
it names was analyzed. The refusal itself and the pilot summary JSON are
unchanged.

The commit clears only the files the preflight names. The preflight skips
deleted files, and files carrying an inline `#[cfg(test)]` module other than
a held inline target, while the full before-phase check may still refuse
them. A start can therefore still refuse after the printed commit; that
refusal stays authoritative and names the remaining files. Closing this gap
belongs to the shared preflight, not to pilot.

## Required Evidence

- `pilot_next_steps_name_the_dirty_target_precondition_only_when_it_applies`
  pins, against a real repository fixture, the line's presence and its position
  between the heading and step 1 for a worktree-dirty and for a staged-only
  inline target (the latter with a `NoChange` current change), its absence at
  HEAD (including `working_tree=true` with another dirty file), for a `tests/`
  target, for an unborn HEAD, and below the Git top-level, and the step 3 wording.
  It compares shared and standalone rendering byte-for-byte and counts actual
  bounded subprocess spawns: five for a committed inline target, two for an
  unborn HEAD, one for a subdirectory root, and zero for a `tests/` target.
  Two standalone renders require twice those counts.
- The `spec_0237` summary helper compares shared and standalone Markdown and
  requires zero Git spawns for its `root = "."` fixtures.
- `generated_workflow_replay_prints_only_runnable_next_steps` keeps the
  Markdown summary free of a placeholder command that cannot run as printed.

- `pilot_names_the_commit_step_before_a_repair_start_that_would_refuse`
  (`crates/ripr/tests/check_repair_agree.rs`) runs the built binary on a
  committed control, an unstaged and a staged production edit, and a dirty
  test file only. The commit step precedes the repair start in the terminal and
  the Markdown summary exactly when the printed start refuses. Its `-C` value
  is the root, and its files equal the refusal's exact `Uncommitted:` set. In
  the unstaged case, running the printed commit lets the printed start begin.
- `pilot_commit_step_merges_the_dirty_inline_target` covers a dirty inline
  target merged, once, into the commit for another dirty production file.
  Running that commit lets the start begin.
- `pilot_at_a_subdirectory_root_prints_no_commit_step_and_no_runnable_start`
  covers three cases. A subdirectory root with a dirty sibling outside it only;
  the same root with dirty files on both sides; and both sides dirty at a
  nested Cargo project the enclosing package does not claim. In each, neither
  surface prints a commit step or a runnable start, both print check's
  subdirectory advisory, and the start itself refuses there.
- `pilot_premise_probe_runs_only_when_a_repair_start_is_printed`
  (`crates/ripr/src/output/pilot/tests.rs`) counts Git spawns. A seam with no
  start keeps zero spawns with the probe enabled; a seam with a start pays
  for exactly one probe, shared by both surfaces.
- `merged_commit_files_adds_the_inline_target_once` and
  `is_outside_root_reads_only_parent_spellings`
  (`crates/ripr/src/output/pilot/render/complete.rs`) pin the merge, its
  dedupe, and the outside-root spelling.

## Non-Goals

- Changing pilot's current-change detection or the pilot summary JSON, which
  stay unchanged.
- Changing the repair start's refusal or the `--edit-authorized` flow.

## Acceptance Examples

1. An inline-module repair target whose file is edited in the worktree or only
   staged: the terminal prints the commit-or-stash line before step 1.
2. A target at HEAD or a `tests/` target: the line is absent.
3. Step 3 reads "run the `continue --attempt ...` command that step 1
   prints".
4. A tracked production file edited (staged or not) while the repair start is
   offered: the commit command precedes the repair start on both surfaces.
5. The same edit under a subdirectory root: no commit command and no runnable
   start; check's subdirectory advisory instead.
6. A dirty file outside a nested standalone project root: no copyable
   command and no runnable start; check's subdirectory advisory instead.

## Test Mapping

- `crates/ripr/src/output/pilot/tests.rs`:
  `pilot_next_steps_name_the_dirty_target_precondition_only_when_it_applies`.
- `crates/ripr/tests/check_repair_agree.rs`:
  `pilot_names_the_commit_step_before_a_repair_start_that_would_refuse`,
  `pilot_commit_step_merges_the_dirty_inline_target`,
  `pilot_at_a_subdirectory_root_prints_no_commit_step_and_no_runnable_start`.
- `crates/ripr/src/output/pilot/tests.rs`:
  `pilot_premise_probe_runs_only_when_a_repair_start_is_printed`.
- `crates/ripr/src/output/pilot/render/complete.rs`:
  `merged_commit_files_adds_the_inline_target_once`,
  `is_outside_root_reads_only_parent_spellings`.

## Implementation Mapping

- `crates/ripr/src/output/pilot/render/complete.rs`: `render_pilot_terminal_with_target`,
  `render_pilot_summary_md_with_target`,
  `render_pilot_human_summaries_with_premise`, `repair_start_prerequisite`,
  `merged_commit_files`, `is_outside_root`,
  `uncommitted_inline_target`, `dirty_target_precondition`.
- `crates/ripr/src/cli/commands/pilot.rs`: the complete pilot summary writes,
  asking the renderer to observe the premise.
- `crates/ripr/src/app/navigation.rs`: `SeamStartPremise`,
  `seam_start_premise`, `commit_prerequisite_command`.
- `crates/ripr/src/output/pilot/commands.rs`: `repair_inline_target_file`.
- `crates/ripr/src/edit_cage/mod.rs`: `inline_target_is_uncommitted`.
- `crates/ripr/src/app/repair_attempt/mod.rs`: `seam_start_dirty_premise`.
- `crates/ripr/src/edit_cage/mod.rs`: `enclosing_git_top_level_above`,
  `exact_git_top_level_above`.

## Metrics

- `unit_test_pass_rate`: the pilot next-step test above is a required Rust
  test; a failure blocks merge.
