# RIPR-SPEC-0003: Agent Context Packet

Status: planned

## Problem

A diagnostic is not enough for a coding agent. The agent needs a compact,
grounded test-writing brief that states what behavior changed, what evidence was
found, what discriminator is missing, and what test shape should be added.

## Behavior

`ripr context --json` should emit a packet optimized for writing one targeted
test.

Finding selection is a prerequisite for both `context` and its `explain`
sibling. A missing location-shaped selector such as `src/lib.rs:abc` or
`:::` should explain `file:line` syntax and a positive decimal line number.
Syntax guidance follows unsuccessful selection; it must preserve exact-ID
matching and location matching. A well-formed missing ID or
location remains a lookup miss. Fresh-analysis recovery must list findings
with the requested root and diff scope so a corrected selector can be retried
from another working directory. Misses remain exit 2 with no machine stdout.
A selector naming several findings (for example two findings at one
`file:line`, or a canonical gap identity shared by grouped findings) refuses
as ambiguous with the bounded candidate finding IDs instead of silently
selecting the first match; ambiguity is the same usage-class refusal as a
miss. A unique canonical gap identity (`gap:...`) resolves exactly its
finding, like a unique finding ID — including the hash form `gap:<16hex>`
that JSON outputs emit for the canonical gap: both spellings of one gap
resolve through the same seam-to-finding binding, on fresh and `--from`
replay paths alike. A seam-shaped selector whose reverse lookup hits a
budget-truncated seam inventory refuses as undecidable instead of rendering
a clean miss the lookup did not establish: the refusal names the
truncation, the wider-budget recovery, and the listing route.

The packet should include:

- task kind
- finding identity
- changed expression or behavior
- owner symbol
- recommended test location
- related existing tests
- fixture or builder hints
- missing input values
- missing oracle shape
- suggested assertion shapes
- confidence
- stop reasons

## Required Evidence

### Unknown finding guidance (#7477)

For `infection_unknown`, `propagation_unknown`, and `static_unknown`, explain
renders `To investigate this uncertainty:` using the same reconciled next step
as JSON findings and context. This is an evidence-resolution experiment, not a
promise that a test changes the static verdict or proof that a test is missing.
Existing producer-specific limitation advice remains authoritative.

Rust guidance follows the actual stop reasons: trace computed inputs (including
default/empty cases), inspect macro output/effects, or inspect a changed
declaration and the behavior consuming it. Inspect what the related test
actually checks without presupposing that an assertion exists. Inspect any
existing assertion before adding or strengthening a test, and do so only when
that inspection establishes a gap. Compare the original and changed
implementations with the project's configured test runner (`cargo test` for
ordinary Cargo tests); custom harnesses can require their own selector arguments.
The static classification may remain unknown. Neither macro expansion nor
fixture/builder value mapping is unlocked by choosing a wider analysis mode or
an undocumented `ripr.toml` key.

Decidable findings retain their concrete `To change the verdict` recipes.
Unknown explain glosses must preserve the domain's meaning: a changed Rust
declaration can lack an analyzed function owner, a macro need not be procedural,
and fixture/async mentions are heuristic limits rather than execution facts.

The context packet should be grounded in a selected finding and include enough
evidence for a human or agent to write one targeted test without broad repo
guesswork.

## Acceptance Examples

```json
{
  "task": "write_targeted_test",
  "finding": {
    "kind": "boundary_gap",
    "changed_expression": "amount >= discount_threshold"
  },
  "recommended_test_location": "tests/pricing.rs",
  "related_existing_tests": ["premium_customer_gets_discount"],
  "missing_input_values": ["discount_threshold"],
  "missing_oracle_shape": ["exact assertion on Quote.total"],
  "suggested_assertions": ["assert_eq!(quote.total, expected_total)"],
  "confidence": "medium",
  "stop_reasons": []
}
```

## Non-Goals

The context packet should not:

- generate complete tests by itself
- claim real mutation outcomes
- include broad unrelated repository context
- omit stop reasons when static evidence is incomplete

## Test Mapping

Planned tests:

- golden context packet for boundary gap
- golden context packet for weak error oracle
- golden context packet for unknown propagation
- LSP copy-context packet equality with CLI context packet
- malformed-location guidance and valid-ID/location retry through both CLI siblings
- unknown explain guidance on three real Rust fixtures across draft/fast/deep/ready,
  with literal finding/class/reason identity, human/JSON section parity, and
  scope-preserving context replay
- retained boundary, missing-assertion, and no-path verdict-change recipes

## Implementation Mapping

Planned modules:

- `domain` context data types
- `app` context use case
- `output::json`
- `lsp` code action bridge

## Metrics

- context packets with related tests
- context packets with missing input values
- context packets with suggested assertion shapes
- context packets with stop reasons when unknown
