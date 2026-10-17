# RIPR-SPEC-0268: Flow-sink evidence row collapses restated labels

Status: proposed

Owner: product-output

Created: 2026-10-16

Linked issues: [#7798](https://github.com/EffortlessMetrics/ripr-swarm/issues/7798).

Support-tier impact: None

The change is render-only: one composed evidence line loses a duplicated
tail. No classification, verdict, evidence class, or support tier changes.

Policy impact: no dependency, network, workflow or security-policy changes.

## Problem

On a `static_unknown` finding, the flow-sink evidence row rendered
`local flow reaches unknown sink: unknown sink (line N)` on both the human
explain surface and the machine JSON `evidence_path`: the kind label
(`FlowSinkKind::Unknown` → `unknown sink`) and the sink text the analyzer
records for an unresolvable sink are the same literal string. The doubled
text read like a leaked placeholder and carried no information beyond the
line number — on the lowest-confidence class, on the surface whose job is
making a verdict trustworthy.

## Behavior

1. **Shared composer.** `output::flow_sink_line::flow_sink_evidence_line`
   is the single composer for the flow-sink evidence row, used by the human
   evidence lines (`output::human::evidence_lines`) and the JSON
   `evidence_path` (`output::json::report`).
2. **Collapse rule.** When the sink text restates the kind label verbatim,
   the `: text` tail is omitted: `local flow reaches unknown sink (line N)`.
   A sink with distinct text keeps the full `kind: text` form
   (`local flow reaches returned value: amount - 10 (line N)`).
3. **No other surface changes.** `FlowSinkFact` values, the
   `FlowSinkKind::label` contract, SARIF rendering, and every other
   consumer of `flow_sinks` are untouched.

## Required Evidence

- Unit tests pin the collapsed form for a restated label and the unchanged
  form for distinct text, on both the shared composer and the JSON
  `evidence_path` path.
- Golden fixtures that render an unresolvable sink re-bless to the
  collapsed row, with every other evidence line byte-identical.

## Non-Goals

- Enriching the unknown-sink text with guessed sink semantics.
- Changing how the analyzer records `FlowSinkKind::Unknown` facts.
- Collapsing any other composed evidence row.

## Acceptance Examples

1. A `static_unknown` finding whose sink text is the literal
   `unknown sink` renders `local flow reaches unknown sink (line 7)` on the
   human explain surface and as the `evidence_path` entry in check JSON.
2. A `return_value` finding with sink text `amount - 10` still renders
   `local flow reaches returned value: amount - 10 (line 3)` — the collapse
   fires only when the text restates the label verbatim.

## Test Mapping

- `output::flow_sink_line::tests::distinct_text_keeps_label_text_tail` and
  `text_restating_label_collapses_duplication` pin the composer rule.
- `output::json::report::evidence_path_separator_tests::unknown_sink_text_restating_label_renders_once`
  pins the machine path.

## Implementation Mapping

- `crates/ripr/src/output/flow_sink_line.rs::flow_sink_evidence_line`.
- `crates/ripr/src/output/human/evidence_lines.rs` and
  `crates/ripr/src/output/json/report.rs::evidence_path_values` call the
  shared composer.

## Metrics

`unit_test_pass_rate`; 37 golden fixtures re-blessed (RIPR-SPEC-0001 output
contract change), no corpus rows moved.
