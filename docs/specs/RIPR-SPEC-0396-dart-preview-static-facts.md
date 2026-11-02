# RIPR-SPEC-0396: Dart Preview Static Facts (package:test, flutter_test, async, and platform boundaries)

Status: proposed

Owner: language-adapter / product-analysis

Created: 2026-11-02

Linked proposal:

- [RIPR-PROP-0001: Multi-Language Adapter Preview](../proposals/RIPR-PROP-0001-multi-language-adapter-preview.md)

Linked ADRs:

- Planned: #5840 (Dart syntax/analyzer/package/Flutter fact substrates)

Linked plan:

- Epic #5591 under programme #5582 (22-ecosystem language expansion)

Linked issues:

- #5839 — this spec leaf (package-test, widget-test, async, platform boundaries)
- Parent #5591 — Dart and Flutter preview evidence epic
- Shared architecture consumed by reference: #5526, #5531, #5532, #5541, #5542,
  #5546, #5559, #5560

Linked PRs:

- None yet

Support-tier impact:

- No tier change. Dart and Flutter remain opt-in preview/advisory until a
  separate support adjudication (#5849) with corpus and receipts. This spec
  does not default-enable Dart, promote gates, badges, baselines, RIPR Zero,
  or stable support. Boundaries remain in
  [support tiers](../status/SUPPORT_TIERS.md).

Policy impact:

- Register this spec in `docs/spec-index.d/`, `policy/doc-artifacts.d/`,
  `.ripr/traceability.d/`, and verdict-corpus `spec-scopes/` fragments.
- Implementation PRs register capability cells in
  `crates/ripr/src/domain/language_capability.rs` and
  `metrics/capabilities.toml` without changing this document's normative
  cell identities.
- No parser dependency, adapter code, routing enablement, or support-tier
  promotion is introduced by landing this spec alone.

## Problem

The Dart/Flutter epic (#5591) needs a normative preview contract before parser,
analyzer, routing, or adapter work (#5840–#5847). Without a written boundary,
implementation can merge package-level `package:test` evidence with
`flutter_test` widget evidence, treat pumps or finder names as observations,
credit async tests without scheduling limits, or infer native/platform reach from
package names.

This spec defines the per-language static-fact contract the future Dart adapter
must produce. The language-neutral router, output metadata, opt-in posture, and
cross-language non-goals live in
[RIPR-SPEC-0026: Language adapter contract](RIPR-SPEC-0026-language-adapter-contract.md).
Shared capability disposition (#5531), witness/output shapes (#5541, #5560),
activation-before-oracle law (#5542), and cross-language edges (#5559,
[RIPR-SPEC-0378](RIPR-SPEC-0378-cross-language-edge-registry-v1.md)) are
consumed by reference, not reimplemented here.

## Behavior

### Posture and wire identity

- Wire language id: `dart`.
- Routed extensions: `*.dart` only (generated/part files follow source-role
  rules below; no separate extension fork).
- Output metadata: `language = "dart"` and `language_status = "preview"` on
  every Dart preview finding unless the shared contract omits preview labels for
  a disabled adapter state.
- Opt-in: Dart runs only when `ripr.toml` lists `dart` under `[languages]
  enabled` and the binary includes a future `lang-dart` feature. Project
  detection may propose Dart when `pubspec.yaml` and `lib/` or `test/` exist,
  but explicit config remains authoritative to disable.
- Zero runtime/tool execution: no `dart`, `flutter`, `pub`, `build_runner`,
  analyzer server, VM, emulator, device, or test/app/plugin execution during
  passive analysis.

### Project and source roles

The adapter classifies repository paths (syntax and marker driven, not build
graph execution):

| Role | Typical paths | Analysis posture |
| --- | --- | --- |
| Pub package root | directory containing `pubspec.yaml` | package name and dependency names are literal facts only |
| `lib/` | package library sources | primary owner/probe substrate |
| `test/` | package tests | test registration and oracle substrate |
| `example/` | examples | optional related-test context; not implied production |
| Generated | `*.g.dart`, `*.freezed.dart`, `*.gr.dart`, `.dart_tool/`, build output markers | excluded or tagged `dart_generated_code` with provenance; not handwritten repair targets |
| Vendor | checked-in third-party under `vendor/` or path dependencies | read-only context; no implicit trust |
| Flutter application | `pubspec.yaml` with `flutter:` section and `lib/main.dart` or Material/Cupertino entry patterns | enables widget-test facts only in widget-test files |
| Flutter plugin | `pubspec.yaml` with `plugin:` platform declarations | platform facts are declarative only |

Monorepos and melos-style layouts may contain multiple package roots; each root
is scoped independently. A plugin folder name, golden declaration, or
`provider`/`bloc`/`riverpod` import cannot establish native behavior, rendering
correctness, or cross-package activation.

### Capability cells (#5531)

Package tests and Flutter widget tests are **separate capability cells**. A
widget-test cell must not inherit support, limits, or non-claims from a
package-test cell, and vice versa.

Normative cell identities (registered in implementation slice #6148 / #5841):

| Cell id | Framework | Mode | Family | Initial disposition |
| --- | --- | --- | --- | --- |
| `dart/package_test/diff/static_facts` | `package_test` | diff | static_facts | `not_proven` until adapter lands |
| `dart/package_test/repo/static_facts` | `package_test` | repo | static_facts | `not_proven` |
| `dart/flutter_test/diff/static_facts` | `flutter_test` | diff | static_facts | `not_proven` |
| `dart/flutter_test/repo/static_facts` | `flutter_test` | repo | static_facts | `not_proven` |
| `dart/package_test/diff/repair_routing` | `package_test` | diff | repair_routing | `not_proven` |
| `dart/flutter_test/diff/repair_routing` | `flutter_test` | diff | repair_routing | `not_proven` |

Each cell carries explicit limits and proof references to this spec until
corpus-backed promotion. Listing a cell in this spec does not compile an
adapter or register runtime cells in `ripr capabilities --json` until the
implementation PR updates the producer table.

### Owner facts and probe identity

Owners the adapter must recognise (syntax-first):

- libraries and compilation units
- classes, mixins, extensions, enums
- constructors (generative, factory, const where visible)
- static and instance methods, getters, setters, operators
- top-level and local functions, function expressions, and tear-offs where
  probe attachment is defined
- async and sync forms (`async`, `async*`, `sync*`)

Stable probe owner identifier:

```text
dart:<normalized/repo-relative/path.dart>::<qualified_owner>
```

Examples: `dart:lib/pricing.dart::applyDiscount`,
`dart:lib/cart.dart::Cart.total`, `dart:lib/models.dart::Invoice`,
`dart:lib/settings.dart::<library>`. Extension and mixin members include the
extension/mixin name in the qualified owner when the changed line maps to that
member.

Owner kinds reuse the shared vocabulary from RIPR-SPEC-0026 where applicable
(`function`, `method`, `class_method`, `module_function`); Dart-specific kinds
are added only when a consumer needs them, with the same fail-closed omission
when unknown.

### Fact families stay distinct

These are separate evidence dimensions; collapsing them is a spec violation:

| Family | Meaning | Must not be inferred from |
| --- | --- | --- |
| `test_registration` | A `test`/`group`/`testWidgets` name is registered | file name, import of `package:test` alone |
| `test_activation` | Setup/teardown/skip/tag/timeout/platform selector makes a test active or inactive | registration alone |
| `async_schedule` | `Future`, `async`/`await`, `Stream`, event-loop timing | completing a test function syntactically |
| `pump_schedule` | `WidgetTester.pump*` / `pumpAndSettle` calls | building a widget in production code |
| `finder_target` | `find.text`, `find.byType`, `find.byKey`, etc. | string/type names in production |
| `interaction` | `tap`, `drag`, `enterText`, `longPress`, semantics actions | finder declaration without executed interaction |
| `observation_sink` | matcher/assertion on finder, semantics, text, widget properties | pump or finder alone |
| `frame_settlement` | `pumpAndSettle`, frame scheduling, `binding.delayed` hooks | single `pump()` without settle policy |

A scheduled or pumped widget test is **not** credited for exposure unless both
an established interaction (when required) and an observation sink that can
discriminate the changed behavior are established on the same related test
body.

### Activation before oracle (#5542)

Oracle eligibility follows the shared activation authority (#5542): a related
test contributes oracle strength only after `test_activation` is established
for that test body (not skipped, not platform-suppressed without resolved
selector, not blocked by unresolved generated registration). Skip, `@Skip`,
`skip:`, `timeout:` without evaluated duration, and platform selectors (`onPlatform`,
`tags` requiring runtime OS features) retain related pointers with relation
`test_activation_unestablished` and emit no repair packet from that test alone.

Setup/teardown (`setUp`, `tearDown`, `setUpAll`, `tearDownAll`) are activation
context, not oracles. They cannot substitute for an assertion on the changed
sink.

### package:test (ordinary package tests)

Registration facts (syntax):

- `test`, `group`, `setUp`/`tearDown`/`setUpAll`/`tearDownAll`
- `tags`, `skip`/`skip:`/`@Skip`, `timeout`, `onPlatform`, retry metadata
- parameterized/table helpers (`test(..., () { ... })` with literal or
  unresolved generated argument lists)

Matchers and expectations (bounded allow-list, fail closed on unknown):

- equality: `expect(a, equals(b))`, `same`, `predicate` with literal closure
  limits as `opaque_custom_assertion_helper`
- type: `isA`, `TypeMatcher`
- throws: `throwsA`, `throwsException`, typed `throws<TypeError>`
- collections: `contains`, ordered/unordered matchers with literal expectations
- async: `completes`, `completion(equals(...))`, `emits`, `emitsInOrder`,
  `emitsAnyOf` — each pairs with `async_schedule` limits when microtask/event-loop
  ordering is not resolved statically
- `expectAsync`, `FakeAsync`, `Clock` overrides — `dart_event_loop_timer` until
  bounded static model exists

Async/Future/Stream behavior:

- An `async` test body or `Future` expectation does not prove the changed owner
  ran before the assertion without a static path from assertion to owner call.
- Stream matchers credit only when the asserted stream is syntactically tied to
  the changed producer; broadcast/transform stages emit `dart_async_scheduling`
  when ordering is unknown.
- Timeouts and `Future.delayed` are not evaluated; they emit scheduling limits,
  not `exposed`.

### flutter_test (widget tests)

Registration:

- `testWidgets` descriptions and `WidgetTester` callback bodies
- `pumpWidget`, `pump`, `pumpAndSettle`, `runAsync`, `binding.scheduleWarmUpFrame`

Finders and interactions:

- Finder construction is `finder_target` only.
- Interaction facts require a resolved finder reference on the same test
  execution path as the interaction call.
- Semantics, text, widget property, and navigation observations are separate
  `observation_sink` kinds (text vs semantics vs `findsOneWidget` vs
  `matchesGoldenFile`).

Rendering and layout:

- No visual correctness, layout goldens, or pixel equality claims.
- `matchesGoldenFile` is a weak or unknown oracle unless independent static
  pairing is defined in a later slice; default is `dart_rendering_visual`.
- Frame settlement requires explicit settle calls or documented bounded pump
  loops; unbounded `pumpAndSettle` with async gaps emits `dart_async_scheduling`.

### Generated code, mocks, and dynamic boundaries

| Condition | `static_limit_kind` (Dart extension) | Classification posture |
| --- | --- | --- |
| build_runner / source_gen outputs | `dart_generated_code` | exclude or provenance-only |
| `dynamic` call or unresolved target | `dart_dynamic_call` | fail closed |
| mirrors, `dart:mirrors`, registry patterns | `dart_reflection_registry` | fail closed |
| mockito/mocktail/fake overrides | `dart_mock_fake` | limit unless call surface stati |
| isolates / `Isolate.spawn` | `dart_isolate` | fail closed |
| timers, `Future.delayed`, event loop | `dart_event_loop_timer` | scheduling limit |
| inherited widget / provider / bloc state | `dart_inherited_widget_state` | limit |
| `MethodChannel`, `EventChannel`, FFI | `dart_platform_channel` / cross-language edge | no reach without #5559 edge |
| native plugin declarations | `dart_native_plugin` | declarative only |
| custom matchers / `expect(..., customMatcher)` | `dart_opaque_custom_matcher` | weak or unknown |
| unresolved package: URI or missing pub context | `dart_unresolved_package_context` | fail closed |

Shared RIPR-SPEC-0026 kinds (`dynamic_dispatch`, `unsupported_syntax`, …)
apply when they are the honest general label; Dart-specific kinds above are
preferred when they carry strictly more accurate non-claims.

### Cross-language and native edges (#5559)

Any Dart↔native, Dart↔Rust/ObjC/Java/Kotlin, or generated binding edge must
appear only as a registered positive edge in
[RIPR-SPEC-0378](RIPR-SPEC-0378-cross-language-edge-registry-v1.md) with
`producer_id` and `registered_producer_fact` basis. Co-located plugin folders,
`pubspec` dependency names, FFI file names, or channel string literals **do
not** mint edges. Consumers fail closed to unrelated subjects when no edge
validates.

### Witness and output reuse (#5541, #5560)

Dart facts project into the shared language-neutral evidence spine and witness
DTOs defined by #5541 and #5560. This spec does not fork JSON schema versions,
SARIF rule ids, or agent packet shapes. Additive optional fields follow
RIPR-SPEC-0026 (`language`, `language_status`, `probe.owner`, `owner_kind`,
`static_limit_kind`, confidence basis fields).

### Classification vocabulary

Findings use conservative static exposure classes only (`exposed`,
`weakly_exposed`, `reachable_unrevealed`, `no_static_path`, `infection_unknown`,
`propagation_unknown`, `static_unknown`). Do not claim runtime test outcomes,
mutation results, or adequacy.

## Required Evidence

Implementation slices must provide:

- Fixture corpus with at least one positive and one discriminating negative per
  major fact family (registration, activation, package matcher, async limit,
  pump, finder, interaction, observation, frame settle, generated exclusion).
- Capability registry rows matching the cell ids in this spec.
- Traceability from this spec id to tests and fixtures.
- `cargo xtask check-spec-format`, `check-spec-numbering`, `check-traceability`,
  `check-doc-artifacts`, `check-doc-index`, and `check-static-language` on
  touched docs.
- Independent false-actionable review corpus (#5848) before support promotion
  (out of scope for this spec-only PR).

This spec-only change proves registry and format gates on the document itself.

## Non-Goals

- No Dart/Flutter analyzer integration, build_runner execution, or VM/test
  execution.
- No default enablement, merge-gate promotion, or support-tier promotion.
- No visual/rendering/platform correctness inference.
- No generated test synthesis or automated edits.
- No adapter/parser/router implementation (follow-on issues #5840–#5847).
- No publication, signing, marketplace, or release action.

## Acceptance Examples

1. **Separate cells:** A repo with only `test/` `package:test` files maps to
   `dart/package_test/*` cells; adding `test/widget_test.dart` with
   `flutter_test` enables `dart/flutter_test/*` cells without upgrading
   package-test disposition.
2. **Pump is not observation:** `testWidgets('x', (tester) async { await
   tester.pumpWidget(MyApp()); });` yields registration + pump facts only;
   classification stays `no_static_path` or activation-limited without
   `expect(find.text('Save'), findsOneWidget)`.
3. **Finder ≠ interaction:** `final btn = find.text('Save');` without `tap`
   does not establish interaction; observation on `btn` still needs a resolved
   finder on the assertion path.
4. **Async limit:** `test('streams', () async { expect later, emits(1)); });`
   with changed owner in a different microtask retains `dart_async_scheduling`
   and does not read `exposed` without static ordering evidence.
5. **Generated file:** A change only in `*.g.dart` is excluded or tagged
   `dart_generated_code`; no handwritten repair target is emitted.
6. **Platform channel:** `MethodChannel('app/channel')` in production does not
   relate to a test importing `services.dart` unless a #5559-registered edge
   connects subjects.
7. **Activation gate:** `@Skip('flaky')` test with strong `expect` retains
   `test_activation_unestablished` and must not supply oracle eligibility.

## Test Mapping

Planned in follow-on slices (#5843, #5844, #5848):

- `fixtures/dart-verdict-corpus/` (authored cases per acceptance example)
- Adapter unit tests under `crates/ripr/src/analysis/language/dart/` (future)
- Policy: `xtask` spec-format/numbering/traceability/doc-artifacts gates on
  this document

See `.ripr/traceability.d/5839-dart-preview-spec-0396.toml`.

## Implementation Mapping

- `docs/specs/RIPR-SPEC-0396-dart-preview-static-facts.md` — normative contract
  (this file)
- Future: `crates/ripr/src/analysis/language/dart/` — adapter implementation
- Future: `crates/ripr/src/domain/language_capability.rs` — cell registration
- Future: `metrics/capabilities.toml` — product capability join (#6148)
- Shared: `crates/ripr/src/provider_contract/cross_language_edge/` — edge law
  (#5559 / RIPR-SPEC-0378)

## Metrics

- `spec_format_compliance`
- `traceability_registration`
- `capability_cell_identity_defined` (design metric; producer registration is
  a later slice)
