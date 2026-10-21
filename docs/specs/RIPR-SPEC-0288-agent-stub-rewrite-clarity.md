# RIPR-SPEC-0288: Agent stub rewrite clarity contract

Status: proposed

Owner: product / cli

Created: 2026-10-20

Linked proposal:

- None yet

Linked ADRs:

- None yet

Linked plan:

- None yet

Linked issues:

- #8891 (split from #8766: a second `ripr agent stub --write` for the same
  seam must report the existing stub and must not misdescribe it)

Linked PRs:

- None yet

Support-tier impact:

- None. The `rust_test_stub` document (schema `0.2`) gains one additive
  `already_present` boolean; no command accepts or executes anything new.
- Claim boundaries remain governed by
  [support tiers](../status/SUPPORT_TIERS.md).

Policy impact:

- No new exposure class, oracle kind, severity, or stage-state value. The
  new JSON field is additive on schema `0.2`; `written` keeps its meaning
  (this run appended the stub).
- Behavior ownership stays with the stub surfaces: stub production and
  placement remain RIPR-SPEC-0003-family / #5471 territory, and the
  agent-brief omission vocabulary remains the brief's own contract. This
  spec records the stub-path rewrite-truth contract those surfaces must
  honor.

## Problem

`ripr agent stub --write` run twice for the same seam taught the wrong
lesson twice:

1. The second run wrote a duplicate test with a `_2` suffix. The stub
   producer's name-collision loop (`unique_test_name`) exists so a
   genuinely different test that happens to share the generated name is
   not shadowed; but a second run for the same seam recomputes
   byte-identical text, so the collision is ripr's own earlier write and
   the honest answer is "the stub is already there", not a second file
   edit.
2. After the first write, the stub's own seam classifies
   `strongly_gripped` (it is exposed: the test ripr wrote already grips
   it). The disclosure reused the agent packet/brief omission wording
   verbatim — "is configured off for strongly_gripped seams and is not
   included in agent results". That is false on the stub path: nothing
   was configured off. The default `Off` seam severity for
   `strongly_gripped` is the agent-brief rendering policy (exposed seams
   are not brief repair targets), not a user configuration, and the seam
   is omitted because its tests already discriminate it — the opposite of
   disabled.

## Behavior

### Already-written detection (#8891 item 1)

When the stub's uniqueness scope (the placement file's current text) already
contains the exact test function ripr would write — same rendered body, same
base test name, modulo line endings — the stub is flagged `already_present`:

- `RustTestStub.already_present` is `true`; `test_name` stays the base name
  and `text` is the identical function.
- A repeated `--write` does not touch the file. Human output says
  `Stub already written at <file>:<line> ...` and still prints the run
  command; the JSON document stays `state: "ready"`, sets
  `"already_present": true`, and keeps `"written": false` (nothing was
  appended this run).
- A name collision with genuinely different content (a user's own test that
   shares the generated name) is unchanged: the name still suffixes
  (`_2`, …) and the write proceeds. Only the byte-identical recompute is
  treated as already written.
- The detection is content comparison, not bookkeeping: it does not matter
  which command or run inserted the text.

### Truthful strongly_gripped disclosure (#8891 item 2)

On the stub path, the grip-class disclosure names the real reason:

- `strongly_gripped` warns `seam <id> is strongly_gripped: existing tests
  already discriminate it, so a stub would add nothing`. The word
  "configured off" must not appear: the default `Off` severity for the
  class is policy, and the classification counterpart (`exposed`) is the
  state, not a suppression.
- A class a user severity actually disables keeps the packet/brief wording
  (`seam <id> is configured off for <class> seams and is not included in
  agent results`): there the config is the real reason.
- The disclosure remains a warning, not a refusal: a `strongly_gripped`
  seam stays `state: "ready"` / exit 0 (RIPR-SPEC-0003-family scaffold
  semantics, #7290).

## Required Evidence

- A second `--write` for the same seam leaves the file with exactly one
  base-named test function, reports "already written" with the run command
  on the human surface, and emits `state: "ready"`,
  `already_present: true`, `written: false` under `--json`.
- A same-named test with different content still receives the `_2` suffix
  and a normal write.
- The `strongly_gripped` disclosure on the stub path names the grip and the
  real reason, never "configured off"; a config-disabled class (for
  example `weakly_gripped = "off"` in `ripr.toml`) still reads "configured
  off".

## Test Mapping

- `crates/ripr/src/analysis/test_stub/tests.rs::recomputed_stub_for_the_same_seam_is_flagged_already_present`
- `crates/ripr/src/analysis/test_stub/tests.rs::same_named_but_different_test_still_suffixes`
- `crates/ripr/src/cli/commands/agent_stub.rs::tests::second_write_for_the_same_seam_reports_already_written`
- `crates/ripr/src/cli/commands/agent_stub.rs::tests::strongly_gripped_stub_names_the_real_reason`
- `crates/ripr/src/cli/commands/agent_stub.rs::tests::config_disabled_class_stub_keeps_the_configured_off_wording`
- `crates/ripr/tests/agent_stub_at_disclosure.rs::second_write_reports_already_written_instead_of_a_duplicate`
- `crates/ripr/tests/agent_stub_compiles.rs::at_kind_stub_discloses_strongly_gripped_classification`
- `crates/ripr/tests/agent_stub_at_disclosure.rs::off_line_predicate_note_precedes_the_grip_class_warning`
  (ordering pin; the grip warning wording it pins is the new reasoned
  form)

## Acceptance Examples

Second `--write` for the same seam, human output (excerpt):

```text
seam <id> is strongly_gripped: existing tests already discriminate it, so a stub would add nothing
Stub already written at src/lib.rs:14 (existing inline module).
Run it: cargo test --manifest-path Cargo.toml clamp_to_boundary_discriminator
```

Same run under `--json` (excerpt):

```json
{
  "schema_version": "0.2",
  "kind": "rust_test_stub",
  "state": "ready",
  "written": false,
  "already_present": true,
  "warnings": [
    "seam <id> is strongly_gripped: existing tests already discriminate it, so a stub would add nothing"
  ]
}
```

A user test named `price_boundary_discriminator` with different body
content still yields `test_name: "price_boundary_discriminator_2"` and a
normal write.

With `weakly_gripped = "off"` in `ripr.toml`, a weakly gripped seam's stub
disclosure still reads `seam <id> is configured off for weakly_gripped
seams and is not included in agent results`.

## Non-Goals

- No change to the agent packet/brief omission vocabulary on the brief and
  packet surfaces themselves; those keep their own wording contract.
- No change to stub placement, selection, or refusal kinds.
- The `OutsideInlineTestRegion ... unparseable` attempt-terminal recovery
  text (a parse error in the agent's new test) lives in the edit-cage
  attempt authority, not the stub path; its missing fix-the-syntax hint is
  recorded in #8891 but intentionally not changed here.

## Implementation Mapping

- `crates/ripr/src/analysis/test_stub.rs` owns `RustTestStub` (new
  `already_present` field) and the detection: `rust_test_stub_for_target`
  renders the base-named function and `scope_contains_test_fn` compares it
  against the uniqueness scope modulo line endings before the
  `unique_test_name` suffix is honored.
- `crates/ripr/src/cli/commands/agent_stub.rs` owns the write gate (skip
  `write_test_stub` when `already_present`), the human "already written"
  line, the additive JSON `already_present` field, and
  `stub_grip_omission_reason`, which names the real reason for
  `strongly_gripped` and defers to the packet/brief wording only for
  classes a user severity actually disables.
- `crates/ripr/src/cli/help/agent.rs`, `docs/AGENT_WORKFLOWS.md`, and
  `docs/OUTPUT_SCHEMA.md` carry the user-visible contract text.

## Metrics

- `unit_test_pass_rate` on the registered tests.
