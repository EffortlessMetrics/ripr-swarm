# RIPR-SPEC-0402: Haskell Preview Package, Component, Example-Test, Property, Laziness, and FFI Boundaries

Status: accepted

Owner: language-adapter / Haskell epic (#5596)

Created: 2026-11-02

Linked proposal:

- None yet. Epic context: programme [#5582](https://github.com/EffortlessMetrics/ripr-swarm/issues/5582).

Linked ADRs:

- Future: parser/GHC/Cabal/Stack substrate ADR (#5888).

Linked plan:

- Epic [#5596](https://github.com/EffortlessMetrics/ripr-swarm/issues/5596) child graph; this spec is leaf #5887.

Linked issues:

- [#5887](https://github.com/EffortlessMetrics/ripr-swarm/issues/5887) (governing)
- [#5596](https://github.com/EffortlessMetrics/ripr-swarm/issues/5596) (parent epic)
- Shared architecture consumed by reference: [#5526](https://github.com/EffortlessMetrics/ripr-swarm/issues/5526), [#5531](https://github.com/EffortlessMetrics/ripr-swarm/issues/5531), [#5532](https://github.com/EffortlessMetrics/ripr-swarm/issues/5532), [#5541](https://github.com/EffortlessMetrics/ripr-swarm/issues/5541), [#5542](https://github.com/EffortlessMetrics/ripr-swarm/issues/5542), [#5546](https://github.com/EffortlessMetrics/ripr-swarm/issues/5546), [#5559](https://github.com/EffortlessMetrics/ripr-swarm/issues/5559), [#5560](https://github.com/EffortlessMetrics/ripr-swarm/issues/5560)

Linked PRs:

- (this document's landing PR)

Support-tier impact:

- No tier change. Haskell remains absent from default `[languages] enabled`,
  absent from support-tier promotion, and absent from blocking gates until a
  separate support adjudication (#5898). This spec defines a preview/advisory
  contract only. See [support tiers](../status/SUPPORT_TIERS.md).

Policy impact:

- Register this spec in `policy/doc-artifacts.d/`, `.ripr/traceability.d/`, and
  `docs/spec-index.d/`.
- Do not register Haskell capability cells, product-capability manifest rows,
  or default enablement in this slice.

## Problem

Haskell projects combine Cabal/Stack packaging, typeclasses, laziness,
higher-order code, property-based tests, Template Haskell, CPP, and FFI in ways
that break imperative assumptions about activation, evaluation demand, and
observation. Without an accepted preview contract, later parser, routing, and
adapter work can collapse distinct concerns:

```text
package identity          ≠ component identity
class method identity     ≠ instance identity
example registration      ≠ property registration
property registration     ≠ known generated example
type signature            ≠ observed value
pure-looking binding      ≠ forced evaluation
foreign import spelling   ≠ registered cross-language edge
```

This spec defines the Haskell preview contract **before** GHC/Cabal/Stack
execution, parser dependencies, routing, or analyzer implementation. Shared
multi-language architecture (#5526 and children) supplies activation,
witness, project, relation, and cross-language edge law; this spec specializes
those authorities for Haskell without reimplementing them.

## Behavior

### Posture and shared authorities

The Haskell preview adapter (future implementation) is opt-in, fail-closed, and
`language_status = preview`. It reuses the language-neutral adapter boundary
([RIPR-SPEC-0026](RIPR-SPEC-0026-language-adapter-contract.md)), preview
disclosure ([RIPR-SPEC-0082](RIPR-SPEC-0082-preview-language-disclosure.md)),
and shared output schemas. It does not fork JSON, SARIF, agent packet, or gate
vocabularies.

Cross-cutting architecture consumed by reference:

| Authority | Issue | This spec uses it for |
| --- | --- | --- |
| Multi-language architecture | #5526 | Adapter boundary, preview posture |
| Witness / relation projection | #5532 | Exact relation stages and ceilings |
| Project / package facts | #5541 | Relocatable package and component identity |
| Activation / oracle eligibility | #5542 | Framework registration vs eligible oracle |
| Actionability / repair law | #5546 | Fail-closed actionability (future) |
| Cross-language edges | #5559, [RIPR-SPEC-0378](RIPR-SPEC-0378-cross-language-edge-registry-v1.md) | FFI and registered bridges only |
| Verification completeness | #5560 | Verification routes consume shared planner (#5162) |

Passive analysis must not execute `ghc`, `cabal`, `stack`, GHCi, property
runners, or native code linked through FFI.

### Wire identity and routing

- Wire language id: `haskell` (stable in config, router, output, and capability
  registry when cells are added later).
- Routed extensions:
  - `*.hs` — primary Haskell source.
  - `*.lhs` — literate Haskell only under a bounded literate contract: Bird-style
    or LaTeX-style markers must be recognized syntactically without executing
    preprocessor or TeX tooling; when the literate role cannot be established,
    emit `static_limit_kind = literate_haskell_unresolved` and refuse positive
    owner facts beyond file presence.
- Not routed as Haskell: `.cabal`, `cabal.project`, `stack.yaml`, `.hsc`, `.chs`,
  `.yaml` build metadata (these inform project detection only through accepted
  substrate facts, not as analyzed Haskell modules).
- Version boundaries: GHC extension and language pragma context is recorded as
  provenance on facts when the substrate exposes it; unsupported or contradictory
  pragma sets yield typed limitations, not silent downgrade to `no_static_path`.

Repo configuration (future) follows RIPR-SPEC-0026: `[languages] enabled` must
name `haskell` explicitly; project detection may propose Haskell only when
Cabal/Stack markers are present and must remain overridable by explicit config.

### Project model: Cabal, Stack, packages, and components

Facts are organized in independent cells (see Capability cells below):

1. **Package identity** — relocatable package id (name + version pin or
   content hash policy defined by substrate ADR #5888), not directory basename
   alone.
2. **Component identity** — library, executable, test suite, benchmark, foreign
   library, sublibrary, or custom component kind as declared in Cabal/Stack
   metadata; components are not interchangeable.
3. **Module identity** — module name + defining component + repo-relative path
   when the substrate ties them; duplicate module names in different components
   remain distinct.
4. **Source role** — each file carries one primary role:
   `library_source`, `executable_main`, `test_module`, `benchmark_module`,
   `vendor_or_submodule`, `generated`, `cpp_conditional`, `ffi_stub`, or
   `unknown_role`.
5. **Generated and vendor** — generated paths (e.g. `Paths_*`, Alex/Happy output,
   TH-spliced artifacts when identifiable without expansion) carry
   `generated=true` and must not donate oracle credit without an accepted
   producer fact tying them to a hand-written owner.
6. **CPP** — CPP branches are separate logical files or tagged regions; facts
   from inactive branches cannot donate evidence for active-branch probes unless
   the substrate proves branch selection.

Stack and Cabal are parallel discovery inputs; conflicting graphs yield
`project_graph_ambiguous` limitations rather than arbitrary winner selection.

### Owner facts and exact probe identity

Owner kinds the adapter must distinguish (never collapse to a single
unqualified symbol):

| Owner kind | Identity must include |
| --- | --- |
| Top-level binding | module + name + binding site span |
| Local / lambda binding | enclosing scope chain + name + span |
| Type, data, newtype | module + type name (+ kind context when known) |
| Data constructor | type + constructor name |
| Typeclass | module + class name |
| Class method | class + method name (not the instance head alone) |
| Instance head | class + instance types (partial applications explicit) |
| Instance implementation binding | instance head + method name + impl span |
| Pattern synonym | module + synonym name + arity |
| Foreign import / export | module + symbol + calling convention + safety |

Stable `probe.owner` shape (illustrative):

```text
haskell:<repo-relative/path.hs>::<module>::<owner-kind>:<qualified-id>
```

Examples:

```text
haskell:src/App/Discount.hs::App.Discount::function:calculateDiscount
haskell:src/App/Discount.hs::App.Discount::class:MonadDiscount
haskell:src/App/Discount.hs::App.Discount::method:MonadDiscount.apply
haskell:src/App/Discount.hs::App.Discount::instance:MonadDiscount(AppM)
haskell:src/App/Discount.hs::App.Discount::foreign-import:c_discount
```

Class methods and instances must not share one owner id. Type signatures alone
do not establish probes; changed lines inside instances attach to the instance
method owner, not the class declaration alone.

### Capability cells (independent dimensions)

Haskell capability is reported as independent cells aligned with
`domain::language_capability` axes (`language/framework/mode/family`). This
spec defines the **semantic contract** for future registration; it does not add
registry rows or manifest claims.

Minimum independent families:

| Family (illustrative id) | Declares |
| --- | --- |
| `haskell/any/diff/static_facts` | Syntax/substrate facts in diff mode |
| `haskell/any/repo/static_facts` | Whole-workspace indexing posture |
| `haskell/cabal/any/project_graph` | Package/component/module discovery |
| `haskell/hspec/example/activation` | Example-test registration and activation (Hspec) |
| `haskell/tasty/example/activation` | Tasty test-tree registration and activation |
| `haskell/hunit/example/activation` | HUnit `TestCase` and assertion registration |
| `haskell/quickcheck/property/registration` | Property registration without generated cases |
| `haskell/hedgehog/property/registration` | Property registration without generated cases |
| `haskell/any/diff/evaluation_demand` | Strict vs lazy demand modeling |
| `haskell/any/diff/effect_boundary` | IO/ST/realWorld and unsafe effect limits |
| `haskell/any/diff/exact_relations` | Exact equation/guard/pattern probes |
| `haskell/ffi/any/registered_edges` | Cross-language edges via #5559 |
| `haskell/any/repo/bounded_mode` | Multi-component completeness disposition |

A property-heavy repository and an example-test-heavy repository must be judged
under separate cells; one green cell must not imply another.

### Example-test framework cells (Hspec, Tasty, HUnit)

Example-based frameworks share activation/oracle law from #5542 but retain
**separate framework cells** (`haskell/hspec/...`, `haskell/tasty/...`,
`haskell/hunit/...`):

- **Hspec** — `describe`/`context`/`it`/`specify`, pending and focus markers,
  filter hooks, and metadata tags are registration facts; they are not execution
  proof.
- **Tasty** — test trees, ingredients, and `TestTree` registration; nested
  groups compose activation but do not merge unrelated leaves.
- **HUnit** — `TestCase` and assertion primitives (`assertEqual`, etc.) as
  syntax-first oracle shapes.

Each discovered test carries:

- `TestActivationFactV1` projection (registration, selection, activation,
  expected-outcome mode, limitations) populated by the Haskell producer.
- `OracleEligibilityV1` per assertion observer, consumed only after activation
  maps through #5542.

Inactive, pending, focused-out, filtered, or quarantined tests may remain related
pointers but cannot donate ordinary reach/oracle credit. Expected-failure and
pending semantics follow #5542: expected failure is not skip; body execution may
still occur with inverted pass semantics.

Oracle families for examples (non-exhaustive):

- equality on values and structures;
- boolean predicates and comparisons;
- exception/assertion primitives (`shouldThrow`, HUnit failure);
- constructor/pattern matching checks;
- collection containment/length/order;
- monadic/IO output and state assertions when demand is established;
- custom assertion combinators — eligible only when the substrate resolves the
  combinator to a known oracle kind; otherwise `opaque_custom_assertion_helper`.

### Property framework cells (QuickCheck, Hedgehog)

Property-based tests are a **separate capability plane** from example tests:

- Registration facts cover property names, groups, labels, classification,
  generator bindings, shrinking hooks, and replay parameters when syntactically
  visible.
- **No statically known generated examples**: registering `prop_*` or a Hedgehog
  `property` does not establish concrete inputs, coverage of a changed owner, or
  a changed-behavior discriminator.
- Generator-to-input flow, discard counts, shrinking trees, and replay seeds are
  `propagation_unknown` or explicit limitations until accepted substrate facts
  model them.
- Property predicates (e.g. `forAll` bodies) may yield predicate-oracle evidence
  only when the changed sink is syntactically reachable in the predicate and
  evaluation demand for the observed expression is established.

QuickCheck and Hedgehog each retain their own framework cell; shared property
vocabulary does not merge their registration state.

### Evaluation demand: exact, lazy, non-strict, effectful

Haskell evaluation order is not optional metadata; it gates observation:

| Demand state | Meaning for static evidence |
| --- | --- |
| `demand_strict` | Substrate establishes strict need for the observed subexpression |
| `demand_lazy_unforced` | Value may be a thunk; observation of inner change is `reachable_unrevealed` or limited |
| `demand_unknown` | Higher-order, opaque wrapper, or unresolved strictness |
| `effect_io_or_realworld` | IO/ST/RealWorld chain present; pure oracle cannot close without effect model |
| `bottom_or_partial` | `undefined`, `error`, partial patterns — no positive discrimination from type alone |

Rules:

- A pure-looking function type does **not** prove a test observes a changed value.
- Constructing a value without forcing it produces at most weak or unreachable
  evidence for inner probes.
- `seq`, `force`, bang patterns, and `$!` may be recognized as strictness hints
  only when syntactically local and substrate-backed; no whole-program strictness
  analysis is claimed.
- Nontermination, `unsafePerformIO`, `unsafeCoerce`, and similar unsafe edges
  carry typed limitations (`unsafe_effect`, `nontermination_unknown`) and fail
  closed for actionability.

### Relations, misses, and FFI

Exact relations (equation, guard, pattern, lazy/effect probes) reuse #5532
witness stages and ceilings. Higher-order flows that cannot resolve callee or
argument demand remain `propagation_unknown` or named static limits.

FFI and native boundaries:

- No positive cross-language edge from foreign symbol spelling, `.hsc` paths, or
  `foreign import` text alone.
- Edges are admitted only through registered producers under
  [RIPR-SPEC-0378](RIPR-SPEC-0378-cross-language-edge-registry-v1.md) / #5559,
  typically `ffi_binding`, `native_addon`, or `manual_registered_bridge`.
- C ↔ Haskell, Haskell ↔ Rust (via C/Haskell FFI), and subprocess CLI boundaries
  remain distinct families with distinct propagation ceilings.
- Package flags (`flags:` in Cabal) and unresolved conditional compilation
  prevent complete component claims until the substrate resolves the flag set
  for the analyzed configuration slice.

### Typed limitations (closed vocabulary extensions)

When facts are insufficient, emit explicit `static_limit_kind` (or successor
limitation DTO) values including at least:

```text
haskell_typeclass_resolution_unknown
haskell_instance_overlap_unknown
haskell_template_haskell_unexpanded
haskell_cpp_branch_unknown
haskell_ghc_rewrite_unknown
haskell_plugin_transform_unknown
haskell_laziness_unobserved
haskell_nontermination_unknown
haskell_higher_order_opacity
haskell_property_generation_unknown
haskell_custom_test_combinator
haskell_package_flag_unknown
haskell_component_incomplete
haskell_ffi_edge_unregistered
literate_haskell_unresolved
```

Limitations are independent: Template Haskell opacity does not excuse missing
activation facts, and property registration does not excuse lazy demand gaps.

### Output, actionability, and verification

- Findings use conservative static exposure vocabulary (`exposed`,
  `weakly_exposed`, `reachable_unrevealed`, `no_static_path`,
  `infection_unknown`, `propagation_unknown`, `static_unknown`).
- Renderers reuse shared witness/output schemas; no Haskell-only public JSON
  fork.
- `repair_packet_ready` stays fail-closed until a future spec proves complete,
  non-dynamic repair shapes for Haskell (out of scope here).
- Verification routes consume the shared planner (#5162); no Haskell-only
  verification planner is introduced.

## Required Evidence

This spec slice is documentation-only. Evidence that the contract is registered:

- `cargo xtask check-spec-format` and `cargo xtask check-spec-numbering` pass
  with the spec file and index fragment present.
- `cargo xtask check-doc-artifacts` and `cargo xtask check-traceability` pass
  with fragment registrations.
- `cargo xtask check-static-language` passes on the spec prose.

Future implementation slices must add discriminating fixtures and tests mapped
under this spec id; until then, planned mappings below are explicit
obligations.

## Non-Goals

- No GHC/Cabal/Stack/test/property/native execution during passive analysis.
- No parser, adapter, router, or capability-registry implementation in this
  slice.
- No default `[languages] enabled` entry, support-tier promotion, gating, or
  publication action.
- No static proof of purity, termination, typeclass laws, or complete instance
  resolution.
- No universal Template Haskell expansion or CPP evaluation.
- No generated tests, edits, or repair packets claimed ready.
- No Haskell verdict corpus or capability manifest rows (later epic children).

## Acceptance Examples

1. **Separate example and property cells** — A repo with both Hspec examples
   and QuickCheck properties reports independent framework cells; disabling
   property analysis does not hide example-test limitations.
2. **Class method vs instance** — A change in an instance's `apply` method
   probes `…::method:ClassName.apply` under instance head `…::instance:ClassName(T)`
   and does not alias the class declaration owner.
3. **Lazy demand** — A test comparing `show (f x)` when `f x` is not forced
   yields at most `reachable_unrevealed` with `haskell_laziness_unobserved`
   unless strict demand is established.
4. **Property registration** — `prop_reverse` registration alone yields no
   known generated example and no `exposed` claim from property name or type
   signature.
5. **Activation before oracle** — A pending `it` block carries activation
   `todo_or_pending`; an equality oracle inside it is ineligible per #5542.
6. **FFI edge** — `foreign import ccall "sin"` creates no C edge without a
   registered `CrossLanguageEdgeV1` producer fact.
7. **CPP** — A probe inside `#ifdef DEV` inactive for the analyzed flag set
   carries `haskell_cpp_branch_unknown` and does not credit active-branch tests.
8. **Shared schemas** — A Haskell preview finding projected to JSON includes
   only additive shared fields (`language`, `language_status`, `probe.owner`,
   `static_limit_kind`, …) without a Haskell-only classification enum.

## Test Mapping

Planned tests:

- `haskell_example_and_property_cells_are_independent`
- `haskell_class_method_and_instance_owner_ids_differ`
- `haskell_lazy_unforced_inner_probe_is_not_exposed`
- `haskell_property_registration_does_not_establish_generated_example`
- `haskell_pending_hspec_oracle_is_ineligible_under_shared_activation`
- `haskell_foreign_import_without_registered_edge_has_no_ffi_relation`
- `haskell_cpp_inactive_branch_carries_cpp_limitation`
- `haskell_preview_finding_uses_shared_output_vocabulary`

## Implementation Mapping

Future slices (see #5596 graph):

- Substrate ADR and parser facts (#5888, #5889).
- Owner/probe production (#5890).
- Hspec/Tasty/HUnit activation (#5892).
- QuickCheck/Hedgehog registration (#5893).
- Relations, demand, and limits (#5894).
- Bounded repo mode (#5895), output parity (#5896), corpora (#5897), support (#5898).

This spec does not map production code in ripr-swarm yet.

## Metrics

- Spec registration gates (`check-spec-format`, `check-spec-numbering`,
  `check-doc-artifacts`, `check-traceability`, `check-static-language`) pass
  for the accepted document.

## Failure Modes

- Treating property registration as a discriminator → false `exposed`; prevented
  by separate property cells and generation limitations.
- Collapsing instance and class method owners → false credit on typeclass-heavy
  code; prevented by distinct owner ids.
- Treating foreign import text as reach → false cross-language repair; prevented
  by #5559 / RIPR-SPEC-0378 admission law.
- Assuming lazy values are observed → false weakly_exposed; prevented by demand
  states.
