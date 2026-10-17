# RIPR-SPEC-0267: Owner call shape projection for drill-in surfaces

Status: proposed

Owner: product-output

Created: 2026-10-16

Linked issues: [#7796](https://github.com/EffortlessMetrics/ripr-swarm/issues/7796).

Support-tier impact: None

The projection adds observed source data to two drill-in surfaces. It
synthesizes no assertion value, changes no verdict, class or support tier,
and declines to the prior behavior whenever the shape cannot be honestly
named.

Policy impact: no dependency, network, workflow or security-policy changes.

## Problem

For a `no_static_path` finding, `ripr explain` ended its verdict-change
recipe with "call `<owner>`, directly or through a helper" — the owner name
alone. The user was told to write a test and simultaneously sent to the
source for the signature, the test-module location, and the expected value.
The producer already owned the answer: `ripr agent stub` derives the typed
call shape from the same parse, so the two surfaces felt inconsistent, and
the honest `suggested_assertion_unavailable` limitation read as if nothing
more could be offered.

## Behavior

1. **Producer.** `analysis::syntax::fn_signature::owner_signature_at_line`
   resolves the innermost `fn` containing the changed line and
   `OwnerSignature::call_shape` renders the observed call shape as written:
   parameter binding names and types, the return type (`()` omits the
   arrow), and, for methods, the self-type qualification with the receiver
   slot (`Bag::push(&mut self, value: i32) -> bool`).
2. **Declines.** The shape is `None` — and both surfaces keep their prior
   behavior — when:
   - the finding is not `candidate_current`: a `base_deleted`,
     `moved_or_renamed`, or `unresolved_subject` finding, or a run whose
     analyzed tree is not the on-disk checkout, cannot borrow a function
     name from a file it does not own;
   - the source file is missing or does not parse cleanly;
   - no function covers the changed line, or sibling functions share the
     changed line and a line-only lookup cannot tell which one owns the
     changed expression;
   - the owner takes a typed `self` receiver, declares unbound type or
     const generics, or sits in a container that cannot be named and called
     from a test (`OwnerContainer::Unsupported` / `GenericImpl`).
3. **Explain surface.** `ripr explain`'s verdict-change recipe names the
   owner by call shape when available
   (`call \`greet(name: &str) -> String\`, directly or through a helper`),
   falling back to the bare name, then to `the changed code`.
4. **Context packet.** The packet probe section carries an optional
   `owner_signature` string with the same shape. The packet version stays
   `1.0`; an absent field is the previous behavior.
5. **Observed data only.** No assertion value, input choice, or expected
   literal is synthesized. The `suggested_assertion_unavailable` limitation
   stands; the stub-template suggestion remains a follow-up on #7796.

## Required Evidence

- `owner_signature_at_line` finds the innermost covering function and
  declines outside any function and for sibling functions sharing a line.
- `call_shape` renders params and return for free functions, self-type
  qualification plus receiver slot for methods, no arrow for `()` returns,
  and declines typed receivers, unbound type generics, and
  unnameable containers.
- The context packet projects the signature for a `candidate_current`
  finding with parseable source, omits it for a missing file, and omits it
  for a `base_deleted` finding even when the file exists.
- The explain recipe test shows the shaped and name-only forms.

## Non-Goals

- Naming the test-module write location in the recipe.
- Offering the `agent stub` template as the suggested assertion.
- Projecting the shape into check, JSON finding, SARIF, badge, or LSP
  diagnostic surfaces.
- Resolving multi-line sibling functions or closure owners by finer
  coordinates than the recorded line.

## Acceptance Examples

1. A `no_static_path` finding on `pub fn greet(name: &str) -> String`
   (candidate-current source): explain's recipe reads
   `call \`greet(name: &str) -> String\` and assert on its result`, and the
   context packet probe section carries
   `"owner_signature": "greet(name: &str) -> String"`.
2. The same finding after the function is deleted on the candidate side
   (`base_deleted`): the recipe falls back to `call \`greet\`` (or the bare
   name fallback) and the packet omits `owner_signature`, even when the file
   still exists on disk with another function at that line.
3. An inherent method `pub fn push(&mut self, value: i32) -> bool` on
   `struct Bag`: the shape is `Bag::push(&mut self, value: i32) -> bool`.
4. `fn identity<T>(value: T) -> T`, a typed `self` receiver, a trait default
   method, or a nested function: no shape is projected anywhere.

## Test Mapping

- `analysis::syntax::fn_signature::tests::{call_shape_renders_observed_params_and_return,
  call_shape_includes_receiver_slot_for_methods, call_shape_omits_arrow_for_unit_return,
  call_shape_declines_unbound_type_generics,
  owner_signature_at_line_finds_the_innermost_covering_function,
  owner_signature_at_line_declines_outside_any_function,
  owner_signature_at_line_declines_sibling_functions_sharing_a_line}` pin the
  producer.
- `domain::context_packet::tests::{context_packet_projects_owner_signature_when_source_names_the_call_shape,
  context_packet_declines_owner_signature_when_source_is_missing,
  context_packet_declines_owner_signature_when_source_is_not_candidate_current}`
  and `output::json::context_packet::tests::{context_packet_renders_owner_signature_when_source_names_the_call_shape,
  context_packet_omits_owner_signature_when_source_is_unavailable}` pin the
  packet.
- `output::human::explain::tests::verdict_change_recipe_carries_the_observed_call_shape_when_known`
  pins the recipe, and
  `cli_smoke::unknown_explain::unknown_explain_repair_preserves_decidable_recipes`
  pins the end-to-end shaped wording for the `all_no_path_disclosure`
  fixture.

## Implementation Mapping

- `crates/ripr/src/analysis/syntax/fn_signature.rs::owner_signature_at_line`,
  `OwnerSignature::call_shape`, `owner_call_shape_at_line`.
- `crates/ripr/src/output/human.rs::render_finding_with_context_command`
  (currentness gate and projection).
- `crates/ripr/src/domain/context_packet.rs::ContextPacketProbe::owner_signature`
  (currentness gate).
- `crates/ripr/src/output/json/context_packet.rs::render_context_packet_dto`
  (optional field emission).
- `docs/OUTPUT_SCHEMA.md` documents the field and its absence semantics.

## Metrics

`unit_test_pass_rate`; no golden, corpus, or metric rows move (the
projection is absent in every rendered golden subject).
