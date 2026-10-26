# RIPR-SPEC-0287: Agent stub names the repair journey it feeds

Status: proposed

Owner: agent-loop

Created: 2026-10-20

Linked proposal:

- None yet

Linked ADRs:

- None yet

Linked plan:

- None yet

Linked issues:

- #8727 (`ripr agent stub` never names the repair journey it feeds)

Linked PRs:

- None yet

Support-tier impact:

- None under [SUPPORT_TIERS.md](../status/SUPPORT_TIERS.md). The `rust_test_stub` document gains one optional block and the
  human output gains two lines; no command accepts or executes anything
  new, and the repair before phase is unchanged.

Policy impact:

- The `rust_test_stub` schema version is `0.3` (`0.2` → `0.3`,
  additive); `docs/OUTPUT_SCHEMA.md`, `docs/EXIT_CODES.md`,
  `docs/AGENT_WORKFLOWS.md`, and the `ripr agent stub --help` text carry
  the same literal.
- No new exposure class, oracle kind, severity, or stage-state value.
  The eligibility flip reuses the existing repair-route authorities
  (`repair_packet_eligibility` plus `recommended_test_is_repair_edit_target`);
  nothing is re-derived or re-interpreted.

## Problem

`ripr agent stub` produces a ready-to-write test for a seam, but its
output never mentions the repair journey that test feeds. To continue
from stub to repair the user must re-derive the entire identity: no
`ripr agent repair --seam-id <id>` start command, no pointer to the
repair packet, no note that the packet's `allowed_edit_surface` is
derived independently of the stub's placement. Worse, nothing checks
that the two independent derivations agree. The stub's placement is
owner-shape-driven (a free crate-root owner can be named from an
integration test; anything else — an `impl` method, a private or nested
function — keeps the stub inline in the production file), while the
repair packet admits the surface its recommended-test authority names.
When the producer's target selection proposes an integration file for a
seam whose owner is not integration-reachable, the stub lands in the
production file and the repair packet admits only the integration file.
The user writes the stub, then the repair before phase refuses at the
dirty-premise gate (tracked production content outside the allowed test
surface) with a message that never mentions the stub.

## Behavior

### `next_repair` on the ready document (#8727)

The `rust_test_stub` schema moves to `0.3`, additive on `0.2`: the ready
document may carry a `next_repair` block, and refused envelopes keep the
`0.2` field set. The block states, for the same seam:

- `start_command`: the documented repair transaction start,
  `ripr agent repair --root <root> --seam-id <id> --phase before`, with
  the root and seam id shell-quoted exactly as the pilot's offered repair
  start quotes them.
- `allowed_edit_surface`: the repair packet's admitted edit surface —
  the same `recommended_test_for` derivation the packet render writes
  into `allowed_edit_surface` and the before-phase edit cage falls back
  to from `recommended_test.file`. The stub's test must land in this file
  for the repair's before phase to accept the edit.
- `placement_divergence`: `{"stub_file": <file>, "packet_file": <file>}`,
  present only when the stub's chosen placement file differs from the
  packet's admitted surface. The divergence is stated at generation time,
  naming both files, instead of failing later at the before phase's
  dirty-premise gate.

The block is present only when the classified seam that produced the stub
also establishes the repair route: the same fail-closed flip the pilot's
offered repair start uses (`repair_packet_eligibility(entry).eligible()`
plus `recommended_test_is_repair_edit_target(entry)`). When the flip does
not hold, or the resolution carries no classified entry at all (a
`--at --kind` file-parse selection), the `next_repair` key stays absent
rather than inventing a route. The projection is computed from the
classified entry the resolver already loaded; no second inventory pass is
made and no packet file is read.

### Human output (#8727)

After the existing `Write it:` / `Run it:` lines, the human output prints
`Next repair: <start_command>`. When the placements diverge, a warning
line follows, naming both files and stating the consequence: writing the
stub where the stub says makes the repair's before phase refuse the edit;
the test belongs in the packet's admitted surface.

With `--write`, that divergence refuses the write instead of warning after
it: the command names both files and the supported route — write the test
in the packet's admitted surface, then run the repair — and touches no
file. The written stub would leave a file dirty outside the admitted
surface, and the repair's before phase would refuse the advertised start
command at the dirty-premise gate; a warning printed after the write
cannot prevent that. Committing the stub does not repair the journey
either, because the packet's edit cage admits only its own surface, so the
refusal is the honest resolution.

## Required Evidence

- A repair-ready stub whose placement agrees with the packet surface
  carries `next_repair` with the repair start command and the admitted
  surface, and no divergence.
- A seam whose producer target selection proposes an integration file
  while the owner is not integration-reachable (for example an `impl`
  method) produces a stub in the production file's inline module and a
  `next_repair` whose `placement_divergence` names the production file
  and the integration file — at stub generation time.
- When the repair route cannot be established (no eligible packet route,
  or a `--at --kind` file-parse selection with no classified entry), the
  ready document omits `next_repair` and the human output prints no
  repair line; no route is invented.
- `agent stub --write` on a seam whose placement diverges from the packet
  surface refuses before writing, names both files and the supported
  route, and leaves the tree untouched.
- The schema version is `0.3` on every ready and refused
  `rust_test_stub` document, and the docs/help literals agree.

## Test Mapping

- `crates/ripr/src/app/test_stub.rs::tests::next_repair_names_the_start_and_the_agreeing_surface`
- `crates/ripr/src/app/test_stub.rs::tests::next_repair_warns_when_the_stub_lands_outside_the_packet_surface`
- `crates/ripr/src/app/test_stub.rs::tests::next_repair_stays_silent_when_the_repair_route_is_not_eligible`
- `crates/ripr/src/app/test_stub.rs::tests::next_repair_stays_silent_when_the_class_is_configured_off`
  (a policy-omitted seam gets no start command, in both human output and JSON)
- `crates/ripr/src/app/test_stub.rs::tests::next_repair_start_preserves_a_literal_backslash_unix_root`
  (the quoted root renders exactly as the filesystem spells it)
- `crates/ripr/src/app/test_stub.rs::tests::at_kind_stub_on_an_open_gap_is_not_strongly_gripped`
  (extended: a `--at --kind` resolution carries no invented route)
- `crates/ripr/src/cli/commands/agent_stub.rs::tests::next_repair_block_carries_the_start_command_and_surface`
  (extended: an agreeing placement omits `placement_divergence` entirely)
- `crates/ripr/src/cli/commands/agent_stub.rs::tests::divergence_warning_names_both_files_and_the_consequence`
- `crates/ripr/src/cli/commands/agent_stub.rs::tests::divergent_write_refuses_before_writing_with_actionable_guidance`

## Acceptance Examples

Ready document, schema `0.3`, for a seam whose producer target selection
proposes `tests/clamp_tests.rs` while the stub lands inline in
`src/clamp.rs`:

```json
{
  "schema_version": "0.3",
  "kind": "rust_test_stub",
  "state": "ready",
  "next_repair": {
    "start_command": "ripr agent repair --root . --seam-id 8f7fa8644fd12280 --phase before",
    "allowed_edit_surface": "tests/clamp_tests.rs",
    "placement_divergence": {
      "stub_file": "src/clamp.rs",
      "packet_file": "tests/clamp_tests.rs"
    }
  }
}
```

Human output tail for the same seam:

```text
Write it: ripr agent stub --root . --seam-id 8f7fa8644fd12280 --write
Run it: cargo test --manifest-path Cargo.toml --lib clamp_to_...
Next repair: ripr agent repair --root . --seam-id 8f7fa8644fd12280 --phase before
Warning: this stub lands in src/clamp.rs, but the repair packet admits only tests/clamp_tests.rs as its edit surface. Write the test in tests/clamp_tests.rs instead, then run the repair; writing the stub to src/clamp.rs makes the repair's before phase refuse the edit.
```

A `--at --kind` file-parse ready document carries schema `0.3` with no
`next_repair` key, and its human output stops after the `Run it:` line.

## Non-Goals

- No new exposure class, severity, oracle vocabulary, or stage state; the
  block reports the seam's existing repair-route facts.
- The stub does not re-derive or weaken the repair packet's readiness
  gates: the block is omitted entirely when `repair_packet_eligibility`
  or the repair-edit-target flip does not hold.
- No repair execution authority moves into the stub path; the block only
  names the documented start command and the admitted edit surface.

## Implementation Mapping

- `crates/ripr/src/app/test_stub.rs` owns `StubNextRepair` and
  `StubPlacementDivergence`; `next_repair_for` projects the block from the
  same classified entry that produced the stub, reusing
  `analysis::repair_route::repair_packet_eligibility`,
  `recommended_test_is_repair_edit_target`, and
  `output::agent_seam_packets::recommended_test_for` (the same derivation
  the packet writes into `allowed_edit_surface`), so the offered surface
  is the file the before-phase cage actually admits.
- `crates/ripr/src/cli/commands/agent_stub.rs` renders the conditional
  `next_repair` JSON block in the ready `rust_test_stub` document (schema
  `0.3`, additive) and the `Next repair:` / divergence warning lines on
  the human surface; `RUST_TEST_STUB_SCHEMA_VERSION` feeds both paths.
- `crates/ripr/src/cli/help/agent.rs` documents the block and the
  omission rule beside the existing strongly_gripped guidance;
  `docs/OUTPUT_SCHEMA.md` and `docs/AGENT_WORKFLOWS.md` carry the
  wire shape and journey wording.

## Metrics

- `unit_test_pass_rate` on the registered tests.
