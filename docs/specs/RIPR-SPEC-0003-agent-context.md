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
with the requested root and explicit diff scope so a corrected selector can
be retried from another working directory. After an implicit dirty-worktree
analysis misses, the first listing intentionally selects committed history,
with the analyzed worktree listing second (RIPR-SPEC-0116). This recovery-only
exception does not change analysis or explicit worktree/diff/artifact choices.
Committed-history recovery explicitly selects
`--committed` on check, explain and context, including when tracked worktree
edits would change the default. A committed check's printed drill-ins and
context's reciprocal explain command retain that choice. Without an explicit
source, the CLI siblings follow check's dirty-worktree default. Explicit
`--worktree`, supplied `--diff`, and verified `--from` replay retain their own
input sources. `--committed` conflicts with those three explicit sources and
refuses before input or root reads. Misses remain exit 2. `explain` keeps stdout
empty; `context` emits JSON only, so a miss writes the standalone `cli_refusal`
document (`code: selector_miss`, #11584) to stdout while the guidance and retry
listing stay on stderr.
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

On a fresh `context` or `explain` lookup miss, retain the check producer's
detected-but-unanalyzed language advisories and non-success language-run
reasons under `Analysis limitations for this run:`. Reuse the producer's
configured-off versus unavailable-binary recovery and TypeScript/JavaScript
family names. This describes the run's scope; capped sample paths do not
establish that a limitation caused this particular selector to miss. Preserve
the selector's syntax/seam guidance and effective root/base/source/mode retry.
Successful selection, ambiguity and ordinary misses without such limitations
retain their existing behavior. Recorded `--from` lookups do not infer these
facts from ambient configuration: the artifact has no complete advisory carrier.
Exit 2 and the prose stderr remain the miss contract (#10720); `context` adds the
`cli_refusal` stdout document, `explain` keeps empty stdout.

The packet should include:

- task kind
- finding identity
- changed expression or behavior
- owner symbol
- recommended test location
- related existing tests
- the matched related-test row count before the packet's display window
- fixture or builder hints
- missing input values
- missing oracle shape
- suggested assertion shapes
- confidence
- stop reasons

## Required Evidence

### Bounded related-test totals (#8952)

The shared packet DTO and JSON renderer retain `Finding::related_tests_total()`
as `related_tests_total` beside the bounded `related_tests` window. Identical
eight-row windows with matched totals of 12 and 13 remain distinguishable.
The count uses the existing retained-count fallback and floor for legacy or
underspecified metadata, before applying the packet's display limit. A zero
display limit preserves the total. Rendering preserves producer evidence,
witnesses, selection and identity algorithms.

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
- `crates/ripr/tests/cli_smoke/selector_language_recovery.rs` — fresh disabled,
  unavailable-binary and enabled-run scope, exact retry replay from a separate
  Git repository, capped samples, mixed-language hits, and artifact non-invention;
  the enabled-Perl no-packet row requires a `lang-perl` test build

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
