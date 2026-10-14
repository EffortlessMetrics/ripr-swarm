# RIPR-SPEC-0261: Agent brief error-variant examples compile and name a runnable command

Status: proposed

Owner: product-output

Created: 2026-10-14

Linked issues: [#7391](https://github.com/EffortlessMetrics/ripr-swarm/issues/7391).

Support-tier impact: None

Policy impact: no dependency, network, workflow or security-policy changes.

## Problem

Agent repair briefs and seam packets carry a copy-ready assertion example
for the changed error variant, plus a `suggested_test_command` the agent can
run. Two defects made that guidance unreliable: an owned string payload was
spliced into pattern position as the raw payload identity, producing an
example that does not compile; and a repair target classified
`not_applicable` emitted `cargo test not_applicable` as the suggested
command — a filter that matches zero tests and exits 0, so the agent could
"verify" against nothing.

## Behavior

- An `error_variant` assertion example is generated from typed parts: an
  owned string payload binds by value and compares against the literal
  (`matches!(err, message if message == "...")`), with the `expect_err`
  label correctly escaped. A payload that cannot appear in pattern position
  (`format!(...)`, another call payload) never serializes a non-compiling
  example as `state: "concrete"`: the assertion shape downgrades to
  `unresolved` with `static_limitation_blocks_derivation`.
- `verification.suggested_test_command` is `null` when the repair target is
  `not_applicable`, instead of emitting a command that matches zero tests.

## Required Evidence

Unit tests cover the owned-string payload example (compilable shape with
the escaped label), the non-pattern payload downgrade to `unresolved` with
the limitation recorded, and the `not_applicable` suggested command reading
`null`. Seam-packet and brief golden fixtures record the emitted example
and command fields.

## Non-Goals

This spec does not change error-variant classification, oracle admission,
or the probe evidence behind the brief. It does not attempt to make every
payload shape derivable: unresolvable shapes stay `unresolved` rather than
emitting a guessed example. It does not change the suggested command for
applicable targets.

## Acceptance Examples

- A changed `Err(format!("port {} busy", port))` payload no longer emits
  `matches!(err, format!("port {} busy", port))` as a concrete example; the
  assertion shape reads `unresolved` and the brief records why.
- A changed `Err("port busy".to_string())` payload emits
  `matches!(err, message if message == "port busy")` with the label
  escaped, compile-ready. Call payloads such as `io::Error::new(..)` stay
  `unresolved`: `error_variant_payload_pattern` accepts only a nameable
  variant path or a well-formed string literal with an optional
  `.to_string()` / `.to_owned()` / `.into()` suffix.
- An agent brief for a `not_applicable` repair target emits
  `"verification": {"suggested_test_command": null}`; the null change is
  rendered by `agent_brief.rs::verification_json`, not the seam packet
  path.

## Test Mapping

- `crates/ripr/src/output/agent_seam_packets.rs::tests::error_variant_assertion_shape_with_owned_string_payload_binds_and_compares_literal`
  covers the owned-string payload example.
- `crates/ripr/src/output/agent_seam_packets.rs::tests::error_variant_assertion_shape_with_format_payload_stays_non_concrete`
  covers the non-pattern payload downgrade.
- `crates/ripr/src/output/agent_seam_packets.rs::tests::wellformed_string_literal_rejects_compound_expressions`
  covers the literal gate.
- `crates/ripr/src/output/agent_brief.rs::tests::verification_omits_suggested_test_command_for_not_applicable_target`
  covers the null suggested command.

## Implementation Mapping

- `crates/ripr/src/output/agent_seam_packets.rs` owns the example
  derivation (including `wellformed_string_literal` and
  `owned_string_literal_payload`).
- `crates/ripr/src/output/agent_brief.rs` renders the brief fields the
  packet supplies, including the `suggested_test_command` emission:
  `verification_json` writes a literal `null` for a `not_applicable`
  repair target instead of a `cargo test not_applicable` command that
  would silently match zero tests.

## Metrics

Report the count of error-variant brief examples that downgrade to
`unresolved` per run; growth in that count means more payload shapes need
derivation coverage, not that downgrade is failing.
