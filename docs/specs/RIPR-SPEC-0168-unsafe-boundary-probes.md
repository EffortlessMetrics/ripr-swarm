# RIPR-SPEC-0168: Unsafe execution boundary probes

Status: proposed

Owner:

Created: 2026-08-29

Linked proposal:

Linked ADRs:

Linked plan:

Linked issues: #3536, #15348

Linked PRs:

Support-tier impact: none

Policy impact: none

## Problem

Candidate changes inside Rust unsafe execution boundaries (`unsafe fn` bodies
and explicit `unsafe {}` blocks) were analyzed exactly like ordinary safe Rust:
the parser discarded the boundary context, and any lexical workaround would
misclassify comments and string literals. A reviewer could not see from the
static evidence that a changed line sits inside an unsafe execution boundary.

## Behavior

The Rust fact index records parser-backed `unsafe_boundary` probe shapes for
`unsafe fn` bodies and explicit `unsafe {}` blocks. When a changed line lies
inside such a boundary, the diff projection can emit an explicit
`static_unknown` probe at the changed line, tagged with the
`unsafe_boundary` probe shape and the `static_unknown` probe family. A line
shared between boundary and non-boundary code projects no unsafe-boundary
probe: the boundary's exact AST byte range must overlap the changed line, and
an opening or closing edge line shared with code outside the boundary
disqualifies it. Nested boundaries resolve to the innermost boundary
containing the changed line. The probe family is `static_unknown` by
construction, so the classifier cannot promote reach plus an oracle into
`exposed` for this family.

A boundary is enclosing for a line whose edit lies in its body, past an
opening brace the removed line carries unchanged, and an edited opener for a
line that carries the opening brace with tokens up to it that differ from the
removed line's, or that has no removed line: the unsafe qualifier, the
signature or the brace changed. A classified probe of any family but
`static_unknown` takes precedence over an enclosing boundary when it carries
the edit on the same candidate line of the same owner: the line reports that
classified probe alone. An unchanged expression beside the edit cannot
suppress the boundary's probe, and an ambiguous replacement keeps it. An
edited opener keeps its own finding beside any classified probe. Each boundary
keeps at most one projection, on its first remaining eligible line.

The probe's expression is the boundary label (`unsafe fn inline_len`,
`unsafe block`), which sets its id and gap id. When the changed line is not the
boundary's opening line, the probe's changed (`after`) text is the trimmed
changed line instead of the label (#14333), so `Changed behavior` names the
edit and pairs with the removed line in `before`. A mid-line `unsafe {}` block
inside an `unsafe fn` (`let repr = unsafe { .. };`) still projects the
enclosing `unsafe fn` boundary when no classified probe carries the edit,
keeping its `unsafe_obligation`; only the displayed changed text moves to the
edited statement. An edited opening line keeps the label as its changed text.

An `unsafe fn` boundary probe, and an `unsafe {}` block whose parsed calls
include `unreachable_unchecked` or `assert_unchecked` (probe label
`unsafe block (unchecked assumption)`), carry the stop reason
`unsafe_obligation` instead of the generic `static_probe_unknown` (#11549).
The reason says the change alters an unsafe contract that safe test code
cannot discharge, and its next step is to check the safety contract and each
caller or assumption site by hand. The class stays `static_unknown`, nothing
is credited, and no test-stub repair is offered. Detection is by parsed call
path, so a string, comment or similarly named item does not match.

That reason belongs to the boundary's own finding. When a classified probe
carries the edit on the same line and that finding is withdrawn, the reason
goes with it: the remaining finding names the edit and its test evidence, and
does not repeat the safety-contract step (#15348).

The call must resolve to the `std`/`core` `hint` (or `intrinsics`) function
through the `use` bindings visible at the call (#12196): a spelled-out or
absolute `std::hint::..` path, an imported or aliased `hint` module, an imported
(possibly renamed) leaf, or a `std::hint::*` glob. A bare name nothing imports,
a function the block defines, a local pattern binding of that name, another
module's import, or a parent module's import (not inherited by a child module)
keeps the plain `unsafe block` label.

## Non-Goals

- Proving unsafe preconditions or replacing unsafe-review processes.
- Modeling file-level unsafe declarations (`unsafe trait`, `unsafe impl`,
  `unsafe extern`, `#[unsafe(...)]` attributes).
- Macro expansion or generated-case enumeration.
- Any change to exposure classification, output schema, support tiers, or
  runtime execution claims.

## Required Evidence

- A golden fixture whose diff changes a line strictly inside an `unsafe {}`
  block that carries no classified probe of its own pins exactly one
  `static_unknown` unsafe-boundary probe at the changed line.
- A changed line inside an unsafe boundary whose classified probe carries the
  edit reports that probe alone: one finding for one edit. An edited opener
  keeps the boundary's own finding, so an edit to an `unsafe fn` signature
  keeps it too.
- Parser and classifier unit tests cover extraction, innermost-boundary
  selection, shared-edge-line fail-closed rejection, and comment/string
  false-positive controls.

## Inputs

- A Rust workspace with a diff that changes a line inside an `unsafe {}` block.

## Outputs

- A classified change alone where it carries the edit inside an enclosing
  boundary; otherwise the innermost boundary's `static_unknown` finding, which
  also stays beside a classified probe that does not carry the edit and on an
  edited opener.

## Acceptance Examples

1. The `unsafe_boundary_probe` fixture: a changed loop body inside an
   `unsafe {}` block produces the boundary probe at the changed line and keeps
   the ordinary value probes for the surrounding function.
2. [abstain] The `unsafe_assumption_stop_reason` fixture: a function that gains the
   `unsafe` qualifier and a function that gains an `unreachable_unchecked()`
   block both read `static_unknown` with stop reason `unsafe_obligation`.

## Test Mapping

- `crates/ripr/tests/unsafe_context.rs` (changed behavior and safety-only controls).
- `fixtures/unsafe_boundary_probe` (golden corpus).
- `fixtures/unsafe_assumption_stop_reason` (golden corpus).
- `crates/ripr/src/analysis/syntax/ra/probe_shape_walk_tests.rs` —
  `probe_walk_labels_unchecked_assumption_blocks_by_parsed_call_only` and
  `probe_walk_resolves_unchecked_assumption_through_use_bindings`.
- `crates/ripr/src/analysis/classify/decision.rs` —
  `unsafe_obligation_requires_static_unknown_family_and_label`.
- `crates/ripr/src/analysis/syntax/ra.rs` — parser extraction tests
  (`ra_adapter_extracts_unsafe_execution_boundaries_without_lexical_false_positives`
  family).
- `crates/ripr/src/analysis/probes/classify.rs` — classifier tests for span
  matching, innermost selection, and edge-line fail-closed guards.

## Implementation Mapping

- `crates/ripr/src/analysis/syntax/ra.rs` — extraction of
  `unsafe_boundary` probe-shape facts (#3516).
- `crates/ripr/src/analysis/probes/classify.rs` — changed-line projection and
  boundary ownership guard; `unsafe_boundary_for_line` decides eligibility and
  whether a boundary is enclosing or an edited opener.
- `crates/ripr/src/analysis/probes/diff.rs` — the per-file projection that
  keeps one finding per boundary and lets a classified probe that carries the
  edit replace an enclosing boundary's finding (#15348).
- `crates/ripr/src/analysis/facts/model.rs` — the
  `unsafe_boundary` variant of the closed `ProbeShapeKind` vocabulary (#5415).

## Metrics

- Golden fixture passes `cargo xtask goldens check`; `unsafe_boundary` appears
  in the fixture corpus denominator.
