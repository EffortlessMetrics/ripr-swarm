# RIPR-SPEC-0284: Agent context packet clarity contract

Status: proposed

Owner: product / output

Created: 2026-10-19

Linked proposal:

- None yet

Linked ADRs:

- None yet

Linked plan:

- None yet

Linked issues:

- #8743 (context packet omits the exposure verdict)
- #8744 (the "unknown sink" evidence row reaches the drill-down with no
  meaning, beside a recipe that names an assertion target)
- #8746 ("Why this verdict" prints the producer phrase "no oracle row")

Linked PRs:

- #8854

Support-tier impact:

- None. The packet gains three fields it always had at the finding level;
  no command accepts or executes anything new.
- Claim boundaries remain governed by
  [support tiers](../status/SUPPORT_TIERS.md).

Policy impact:

- The packet contract version is registered in `policy/output_contracts.txt`
  as `context_version` and was bumped `1.0` → `1.1`; `docs/OUTPUT_SCHEMA.md`
  carries the same literal and the packet example.
- No new exposure class, oracle kind, severity, or stage-state value. The
  verdict fields are wire-identical to the `ripr check --json` finding
  fields of the same names.
- Behavior ownership stays with the surface specs: the flow-sink row and
  its gloss belong to RIPR-SPEC-0268 / RIPR-SPEC-0122, and the
  related-test verdict label belongs to RIPR-SPEC-0224. This spec records
  the packet contract and the cross-surface consistency obligations those
  changes created.

## Problem

`ripr context --json` is the machine-readable drill-in for one finding,
marketed as "always JSON, for an agent or tool to consume". Three defects
made the packet and its sibling surfaces teach the wrong lesson:

1. The packet never stated the exposure verdict. It carried the five
   `ripr` stage-state strings but not the finding's `classification`,
   `severity`, or `confidence` — the three headline facts `ripr check
   --json` leads with. The human renderer documents the trap: a
   `discriminate: yes` grade does not mean a discriminator exists, so a
   consumer holding only the packet could state the verdict only by
   guessing from signals ripr says are not the verdict.
2. `local flow reaches unknown sink (line N)` was the only unglossed
   internal term in the explain vocabulary, and the same render could
   carry a "To change the verdict" recipe naming an assertion target
   shape while the propagation evidence said the sink is unclassified.
3. A related-test row whose assertion text was not retained rendered its
   verdict as the producer phrase `no oracle row` — record vocabulary
   that contradicts the assertion a user finds in the source.

## Behavior

### Packet contract 1.1 (#8743)

The context packet version is `1.1`. The packet states, between
`source_currentness` and `probe`:

- `classification`: `ExposureClass::as_str()`, wire-identical to the
  `ripr check --json` finding field. Consumers must not derive the class
  from the underdetermined `ripr` stage states.
- `severity`: `ConfigSeverity::as_str()` resolved through the caller's
  `RiprConfig` (`SeverityConfig::for_exposure`), so a user severity
  override applies on the packet surface exactly as it does on the check
  surface. `ContextPacket::from_finding` seeds the class profile default
  for callers that hold no config; every renderer that holds a config
  re-resolves it.
- `confidence`: the finding's confidence float, wire-identical to the
  check finding field.

The context refusal envelope (RIPR-SPEC-0003 selector/refusal family) is
keyed with the packet's own version, so the bump moves the envelope key
to `1.1` as well.

The `ripr` stage-state block remains in the packet unchanged: it is
still evidence, and it is still not the verdict.

### Unknown-sink meaning and provisional recipe (#8744)

The human evidence window (RIPR-SPEC-0122 surfaces: `ripr explain` and
`check --format human-full`) appends a meaning to the `unknown sink` row:
"ripr could not classify where the changed value ends up; propagation
confidence is limited". The JSON evidence strings stay raw — the machine
shape does not absorb prose. The meaning owner is
`flow_sink_kind_meaning`, the sink-row analogue of `stop_reason_meaning`;
classified kinds need no gloss because their labels are already plain
words.

When the only flow-sink evidence is an unclassified sink, the
verdict-change recipe list gains one qualifier line: "treat the assertion
target as provisional: ripr could not classify where the changed value
ends up". The recipes remain — they are still the best static guess —
but they no longer read as a settled assertion target ripr said it could
not classify. The behavior owner remains RIPR-SPEC-0268's evidence row;
this spec records the cross-surface rule that a recipe must not claim a
target shape the evidence says is unclassified.

### Assertion-recording label (#8746)

In "Why this verdict", a row whose assertion text ripr did not retain is
labelled `assertion not recorded` — a retention fact, not evidence the
test lacks an oracle, consistent with how `ObservationUnconfirmed` is
labelled an "unconfirmed" rather than a miss (RIPR-SPEC-0224 family).
When row grouping means another row of the same test printed its
assertion, the bare fallback is suppressed entirely; that sibling row
keeps its own verdict as the note on its `checked:` line.

## Required Evidence

- The packet emits `classification`, `severity`, and `confidence` on
  version 1.1 for every finding class, and `severity` follows a user
  config override on both the packet and the check surface of one run.
- The unknown-sink row carries its meaning on the human evidence window
  and the JSON evidence strings stay raw.
- The verdict-change recipe is qualified exactly when the only flow-sink
  evidence is unclassified.
- A retained row with no assertion text reads `assertion not recorded`,
  the old producer phrase is gone, and the bare fallback is suppressed
  when a sibling row printed an assertion.

## Test Mapping

- `crates/ripr/src/output/json/context_packet.rs::tests::context_packet_states_verdict_fields_for_packet_consumers`
- `crates/ripr/src/output/json/context_packet.rs::tests::context_packet_severity_follows_caller_config_override`
- `crates/ripr/src/domain/context_packet.rs::tests::context_packet_from_finding_carries_probe_and_ripr_shape`
- `crates/ripr/src/lsp/tests.rs` copy-context packet equality (CLI/LSP
  wire parity under the session config)
- `crates/ripr/tests/cli_smoke.rs` replay refusals and packet-emission
  pins, keyed to the 1.1 contract
- RIPR-SPEC-0268 / RIPR-SPEC-0224 test entries for the gloss, recipe
  qualification, and retention-label rows

## Acceptance Examples

Packet version 1.1 for a `weakly_exposed` finding with confidence 0.75, no
severity override:

```json
{
  "version": "1.1",
  "tool": "ripr",
  "source_currentness": "candidate_current",
  "classification": "weakly_exposed",
  "severity": "warning",
  "confidence": 0.75,
  "probe": { "id": "probe:src_lib.rs:predicate:bbaa2c25" },
  "ripr": { "reach": "yes", "infect": "weak", "propagate": "yes", "observe": "yes", "discriminate": "weak" }
}
```

A user override `severity.weakly_exposed = "note"` in `ripr.toml` renders
`"severity": "note"` on this packet and on the `ripr check --json` finding
for the same run — one resolution, both surfaces.

Unknown-sink evidence window row, human surface only:

```text
  - local flow reaches unknown sink (line 3) — ripr could not classify where the changed value ends up; propagation confidence is limited
```

Verdict-change recipe list when that unclassified sink is the only
flow-sink evidence:

```text
  To change the verdict, a test needs to:
  - call `discount(amount: i32, discount_threshold: i32) -> i32` and assert on its result
  - treat the assertion target as provisional: ripr could not classify where the changed value ends up
```

Related-test rows in "Why this verdict":

```text
  - tests/parse.rs:7 parses_empty: assertion not recorded
```

and, when another row of the same test printed its assertion, the bare
fallback is suppressed:

```text
  - tests/parse.rs:7 parses_empty
      checked: assert_eq!(parse("a"), Ok(1)) (weak exact value oracle)
```

## Non-Goals

- No new exposure class, severity, oracle vocabulary, or stage state.
- The packet does not synthesize a verdict when the classifier could not
  establish one; the static-unknown classes remain static-unknown.
- The gloss and the label are human-surface wording only; JSON evidence
  strings and packet fields stay producer values.

## Implementation Mapping

- `crates/ripr/src/domain/context_packet.rs` owns the packet DTO; the
  version literal, `classification`, `severity` (class profile default),
  and `confidence` are set in `ContextPacket::from_finding`.
- `crates/ripr/src/output/json/context_packet.rs` owns the renderer and
  `CONTEXT_PACKET_VERSION_CONTRACT`; renderers holding a `RiprConfig`
  re-resolve `severity` through `SeverityConfig::for_exposure`.
- `crates/ripr/src/app/context.rs`, `crates/ripr/src/lsp/backend.rs`, and
  the MCP parity route thread the effective configuration through the one
  renderer; the refusal envelope in `crates/ripr/src/cli/commands/context.rs`
  is keyed with the same contract constant.
- `crates/ripr/src/output/flow_sink_line.rs` owns `flow_sink_kind_meaning`;
  `crates/ripr/src/output/human/evidence_lines.rs` appends it on the human
  evidence window; `crates/ripr/src/output/human/explain.rs` owns the
  provisional-recipe qualifier and the `assertion not recorded` label.
- `policy/output_contracts.txt` registers `context_version|1.1`;
  `docs/OUTPUT_SCHEMA.md` carries the matching literal and example.

## Metrics

- `unit_test_pass_rate` on the registered tests.
