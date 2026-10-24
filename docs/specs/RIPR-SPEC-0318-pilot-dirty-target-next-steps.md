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

### Rule 2: continue command

The terminal's final step and the Markdown summary's repair introduction name
the `continue --attempt ...` command that step 1 prints, not an `--attempt ...
--phase after` spelling that step 1 never prints.

## Required Evidence

- `pilot_next_steps_name_the_dirty_target_precondition_only_when_it_applies`
  pins, against a real repository fixture, the line's presence and its position
  between the heading and step 1 for a worktree-dirty and for a staged-only
  inline target (the latter with a `NoChange` current change), its absence at
  HEAD, and for a `tests/` target, and the step 3 wording.
- `generated_workflow_replay_prints_only_runnable_next_steps` keeps the
  Markdown summary free of a placeholder command that cannot run as printed.

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

## Test Mapping

- `crates/ripr/src/output/pilot/tests.rs`:
  `pilot_next_steps_name_the_dirty_target_precondition_only_when_it_applies`.

## Implementation Mapping

- `crates/ripr/src/output/pilot/render/complete.rs`: `render_pilot_terminal`,
  `render_pilot_summary_md`, `uncommitted_inline_target`,
  `dirty_target_precondition`.
- `crates/ripr/src/output/pilot/commands.rs`: `repair_inline_target_file`.
- `crates/ripr/src/edit_cage/mod.rs`: `inline_target_is_uncommitted`.

## Metrics

- `unit_test_pass_rate`: the pilot next-step test above is a required Rust
  test; a failure blocks merge.
