# RIPR-SPEC-0395: Apple shared project facts and separate Swift and Objective-C evidence contracts

Status: proposed

Owner: language-adapter / apple epic

Created: 2026-11-02

Linked issues:

- [#5682](https://github.com/EffortlessMetrics/ripr-swarm/issues/5682) (this contract)
- [#5590](https://github.com/EffortlessMetrics/ripr-swarm/issues/5590) (Apple epic)
- [#5582](https://github.com/EffortlessMetrics/ripr-swarm/issues/5582) (22-ecosystem programme)

Shared architecture consumed by reference (not implemented here):

- [#5526](https://github.com/EffortlessMetrics/ripr-swarm/issues/5526) multi-language architecture
- [#5531](https://github.com/EffortlessMetrics/ripr-swarm/issues/5531) capability cell registry
- [#5532](https://github.com/EffortlessMetrics/ripr-swarm/issues/5532) language routing and feature gates
- [#5541](https://github.com/EffortlessMetrics/ripr-swarm/issues/5541) test registration and activation
- [#5542](https://github.com/EffortlessMetrics/ripr-swarm/issues/5542) test/oracle state vocabulary
- [#5546](https://github.com/EffortlessMetrics/ripr-swarm/issues/5546) limitation and authority law
- [#5559](https://github.com/EffortlessMetrics/ripr-swarm/issues/5559) cross-language bridge edges
- [#5560](https://github.com/EffortlessMetrics/ripr-swarm/issues/5560) repository completeness posture

Linked specs:

- [RIPR-SPEC-0026](RIPR-SPEC-0026-language-adapter-contract.md) language-neutral adapter boundary
- [RIPR-SPEC-0378](RIPR-SPEC-0378-cross-language-edge-registry-v1.md) cross-language edge registry v1

Follow-on implementation graph (out of scope for this document):

- #5683 ADR substrate boundaries
- #5684–#5694 analysis, evidence, output, and eval slices in epic #5590

Support-tier impact:

- No enablement, gating promotion, badge, baseline, RIPR Zero, release, signing, or
  publication authority. Swift, Objective-C, Swift Testing, XCTest, UI/performance tests,
  Objective-C++, and each Apple platform/SDK/configuration context remain
  `preview` / `not_proven` until separate promotion PRs cite receipts. The canonical
  boundary remains [support tiers](../status/SUPPORT_TIERS.md).

Policy impact:

- Register in `policy/doc-artifacts.d/`, `docs/spec-index.d/`, and
  `.ripr/traceability.d/`.
- Register planned capability cells in Appendix B (machine-readable TOML); #5684
  copies them into `domain/language_capability.rs` without widening semantics.
- No adapter, parser, routing, or Rust producer code in the spec PR.

## Problem

Epic #5590 needs one Apple **project substrate** shared by Swift and Objective-C
across Swift Package Manager and bounded Xcode discovery, while keeping language
semantics, test frameworks, bridge facts, and platform/configuration context
separate end to end.

Shared targets, products, headers, module maps, and bridging boundaries must not
make Swift and Objective-C interchangeable. Mixed projects must retain language,
target, platform, SDK, deployment, configuration, architecture, compiler/preprocessor,
generated/source role, framework, and bridge provenance on every claim.

Without a normative contract, adapter work can collapse into proximity heuristics
(imports, selectors, target membership, linkage) or execute `Package.swift`, resolve
dependencies, or launch Xcode merely to obtain passive facts.

## Behavior

### Contract posture

This spec defines **evidence law and portable fact shapes** only. Producers named in
#5683–#5690 must emit facts that validate against the schemas below. RIPR consumes
them through the existing language-neutral spine ([RIPR-SPEC-0026](RIPR-SPEC-0026-language-adapter-contract.md))
with `language` ∈ `{swift, objective_c}` and `language_status = "preview"`.

Initial disposition for every capability cell in Appendix B is `not_proven`. A cell
documents what may eventually be claimed and what proof must exist; it does not imply
implementation.

### Authority law (summary)

Facts inherit the shared limitation vocabulary (#5546) and cross-language registry
rules ([RIPR-SPEC-0378](RIPR-SPEC-0378-cross-language-edge-registry-v1.md)):

- Target membership, imports, header/module-map proximity, linkage, selector spelling,
  symbol names, or co-location **do not** establish executable owner reach, test reach,
  or positive bridge relations.
- Positive Swift↔Objective-C relations require a registered edge with
  `boundary_family` ∈ `{ffi_binding, generated_binding, manual_registered_bridge}` and
  producer-registered basis through #5559.
- Unresolved overload, generic, protocol witness, extension, dynamic dispatch, selector,
  actor/concurrency scheduling, macro/result-builder expansion, reflection, KVC/KVO,
  swizzling, forwarding, preprocessor/configuration/SDK ambiguity, or generated test
  rows must surface typed limits; they must not become universal claims.
- Test registration, selection, activation, skip/expected-failure/known-issue,
  parameterization, trait/tag/filter, fixture binding, async/expectation state, and
  oracle eligibility remain distinct (#5541, #5542). Registration precedes oracle credit.

`Package.swift` is executable Swift. Baseline analysis may consume only an accepted
deterministic static subset or a bounded fact packet; it **must not** execute the
manifest, resolve or download dependencies, or inherit host SwiftPM state.

### Shared Apple project substrate

Portable project facts are language-neutral containers. They carry **no** Swift or
Objective-C owner semantics.

#### Covered inputs (repository-local, static)

**SwiftPM (`Package.swift` static subset or fact packet):**

- package identity, tools version, platforms/deployment declarations;
- products, targets, test targets, executable/tool targets;
- target dependencies, resources, local package relationships;
- conditional platform settings only when textually explicit in the accepted subset.

**Xcode (bounded `.xcodeproj` / `.xcworkspace` parse, no build execution):**

- projects/workspaces, targets, build configurations;
- schemes and test plans **only** where repository-local static models exist;
- source/test bundles, build settings that affect compilation conditions;
- generated file roles, bridging headers, module maps, headers, resources;
- test-framework signals (linked frameworks, build phases) without executing tests.

**Membership and conditions:**

- target membership, platform/SDK/deployment/configuration/architecture conditions;
- generated vs authored vs vendor source roles;
- exact repository-selected analysis root and currentness digest.

#### Shared fact packet (`apple-project-facts-v1`)

Top-level JSON (repo-relative paths, `/` separators, deterministic sort keys):

```json
{
  "schema_version": "apple-project-facts-v1",
  "packet_id": "apple-project:sha256:…",
  "packet_status": "complete|partial|unavailable",
  "root": { "repo_relative": ".", "selected_root": "Packages/MyFeature" },
  "swiftpm": {
    "packages": [],
    "products": [],
    "targets": [],
    "test_targets": [],
    "local_packages": [],
    "tools_version": "6.0",
    "platforms": []
  },
  "xcode": {
    "projects": [],
    "workspaces": [],
    "targets": [],
    "configurations": [],
    "schemes": [],
    "test_plans": [],
    "build_settings": [],
    "generated_files": [],
    "bridging_headers": [],
    "module_maps": []
  },
  "limitations": []
}
```

Each `target` record includes stable ids:

```text
apple:swiftpm:<package>:target:<name>
apple:xcode:<project>:target:<name>:config:<configuration>
```

Bridge records reference #5559 edges only; they never infer relations from headers alone:

```json
{
  "edge_id": "bridge:…",
  "boundary_family": "manual_registered_bridge",
  "swift_owner_ref": "swift:…",
  "objc_owner_ref": "objc:…",
  "bridge_kind": "bridging_header|generated_interface|module_map|objc_attribute",
  "source_span": { "path": "…", "start_line": 1, "start_column": 1, "end_line": 1, "end_column": 1 }
}
```

### Swift evidence contract

Swift facts are emitted only for `.swift` sources (and macro expansion regions only
when a bounded expansion map exists). Identities are **signature- and context-aware**:
module, target, platform, configuration, and compiler context participate in owner ids.

#### Owner kinds (non-exhaustive closed registry for v1)

| Kind | Identity must include |
| --- | --- |
| module | module name, target ref, configuration context |
| type | struct/class/enum/protocol/actor name, generic signature when present |
| extension | extended type, visibility, where clauses |
| member | name, overload signature, isolation (`@MainActor`, `nonisolated`) |
| accessor | property/subscript, getter/setter/willSet/didSet |
| initializer/deinitializer | parameter list, failable/designated context |
| macro/result-builder region | macro name or builder attribute, expansion span when known |

Stable owner id shape:

```text
swift:<repo/path.swift>::<qualified_owner>@<signature_digest>@<target/platform/config digest>
```

Probe families attach to **exact spans** inside the owner body or signature per
RIPR-SPEC-0026 probe vocabulary. Overloads, opaque types, existentials, protocol
witnesses, dynamic dispatch, async/await, actors, tasks, continuations, typed throws,
availability, and macro/generated regions each carry explicit limitation kinds when
static resolution is incomplete (Appendix A).

Swift and Objective-C interop facts cite bridge edges; `@objc` exposure alone is not
a bridge edge without #5559 registration.

### Objective-C evidence contract

Objective-C facts cover `.m`, `.h`, and bounded `.mm` only when an explicit
Objective-C++ capability cell is promoted (Appendix B); `.m` support does not imply
Objective-C++.

#### Owner kinds

| Kind | Identity must include |
| --- | --- |
| interface/implementation | `@interface` / `@implementation` name, super/protocol list |
| category/class extension | base class, category name, method list provenance |
| protocol | optional/requirement set as declared |
| method | selector spelling, instance vs class, type encoding when present |
| property | attribute list, readonly/readwrite, ivar synthesis hints |
| function | C linkage functions in ObjC translation units |
| block typedef | block signature |

Stable owner id shape (repo-relative path preserves the actual `.m`, `.h`, or promoted `.mm`
extension):

```text
objc:<repo/path/with/actual/extension>::<class_or_category>::<selector_or_symbol>@<preprocessor/target/config digest>
```

Selectors, categories, method replacement, optional protocol methods, nullability,
availability, forwarding, associated objects, blocks, and GCD calls require exact
declaration/implementation provenance. Dynamic messaging, runtime lookup, swizzling,
KVC/KVO, and forwarding without static targets emit typed limits (Appendix A).

### Test and oracle contracts (separate producers)

Three framework producers remain **independent**. Facts must not merge Swift Testing
with XCTest or Swift with Objective-C XCTest oracles.

#### Shared test state machine (#5542)

States are distinct and ordered for eligibility:

```text
registered → selected → active → oracle_eligible
```

Parallel terminal/branch states include:
`disabled`, `skipped`, `platform_limited`, `availability_limited`, `trait_filtered`,
`tag_filtered`, `parameterized_generated`, `known_issue`, `expected_failure`,
`fixture_bound`, `async_expectation_bound`, `ui_only`, `performance_only`, `unknown`.

Test target membership alone is **not** reach. Assertions, expectations, confirmations,
interactions, and framework macros receive credit only through exact framework identity
and a bound observed sink (#5541).

#### Swift Testing (`import Testing`)

Map suites/tests with registration metadata:

- `@Test`, `@Suite`, traits/tags/conditions, parameterized arguments, known issues,
  confirmations, exit/error/value observations, async context when statically visible.

Oracle strength examples:

| Construct | Strength when sink bound |
| --- | --- |
| `#expect(a == b)` on changed sink | strong exact |
| `#require(…)` | strong exact (fail-fast) |
| `#expect(throws: …)` | strong error-path when error type/message pinned |
| `withKnownIssue { … }` | known-issue path; not uncredited failure |

#### Swift XCTest (`import XCTest`)

Map `XCTestCase` subclasses, `test*` methods, lifecycle hooks, skip/continue,
expected failures, async tests, expectations/waiters, performance/UI boundaries.

| Construct | Strength when sink bound |
| --- | --- |
| `XCTAssertEqual` / `XCTAssertNotEqual` on changed sink | strong exact |
| `XCTAssertTrue/False`, nil/non-nil | weak unless operand pins changed sink |
| `XCTWaiter`, `wait(for:timeout:)` | requires established async activation |

#### Objective-C XCTest

Same XCTest vocabulary with selector/receiver identity on assertions and expectations.
UI and performance tests map to `ui_only` / `performance_only` states unless a
separate capability cell is promoted.

### Limitation and authority law (Apple extensions)

Appendix A lists closed `static_limit_kind` values for Apple adapters. Producers must
emit them instead of silent `no_static_path` when the reason is known.

Representative limits:

- `apple_package_manifest_unexecuted` — manifest subset only; no runtime SwiftPM.
- `apple_xcode_build_not_run` — project files parsed; no scripts/plugins/macros/builds.
- `swift_overload_unresolved`, `swift_witness_unresolved`, `swift_actor_isolation_unresolved`
- `swift_macro_expansion_unresolved`, `swift_result_builder_unresolved`
- `objc_selector_dispatch_unresolved`, `objc_category_replacement_unresolved`
- `objc_swizzling_detected`, `objc_forwarding_unresolved`
- `apple_sdk_availability_unresolved`, `apple_preprocessor_context_unresolved`
- `apple_bridge_edge_missing` — cross-language claim without #5559 edge
- `apple_objcpp_mode_unpromoted` — `.mm` or C++ constructs without Objective-C++ cell
- `apple_ui_test_unpromoted`, `apple_performance_test_unpromoted`

### Machine-readable capability index

Appendix B registers every language × framework × analysis mode × capability family ×
platform context cell for the Apple epic. Cells use `support_state = "not_proven"` and
`tier_ceiling = "preview"` until #5694 adjudication.

Doc/traceability registration for this spec:

- Appendix B TOML is the authoritative planned cell list for #5684.
- `docs/spec-index.d/RIPR-SPEC-0395.md` indexes this spec.
- `.ripr/traceability.d/5682-apple-evidence-contract.toml` links the spec to format gates.

## Required Evidence

This spec PR proves registration only:

- `cargo xtask check-spec-format`
- `cargo xtask check-spec-numbering`
- `cargo xtask check-doc-artifacts`
- `cargo xtask check-doc-index`
- `cargo xtask check-traceability`
- `cargo xtask precommit` (docs/policy delta)

Follow-on producers (#5685–#5690) must add:

- fixture corpora with positive/negative examples listed under Acceptance Examples;
- registry unit tests for Appendix schemas;
- capability cell pins copying Appendix B into the binary registry;
- judged corpora (#5694) before any promotion discussion.

## Non-Goals

- Executing SwiftPM, Xcode, compilers, macros, plugins, simulators, devices, tests, or apps.
- Dependency resolution/download, signing, notarization, marketplace, or publication.
- Default language enablement or support-tier promotion.
- Proving runtime bridge, concurrency, UI, or mutation outcomes.
- Collapsing Swift and Objective-C semantics because they share a target.

## Acceptance Examples

### Positive — SwiftPM target and configuration facts

Given a `Package.swift` declaring iOS 17 and a library target `Core` with a test target
`CoreTests`, the project packet lists both targets with platform/deployment facts and
does not execute the manifest. A changed Swift function in `Sources/Core/Foo.swift`
under target `Core` carries owner id digest including `Core` and iOS 17 context.

### Negative — manifest execution refused

Given analysis configured for SwiftPM facts, the producer must not spawn `swift package
resolve` or write `.build`. Missing static parse of a manifest emits
`apple_package_manifest_unexecuted` plus `partial` packet status rather than invented targets.

### Positive — bridging header edge (#5559)

Given a bridging header `Project-Bridging-Header.h` importing `LegacyObjc.h` and a
registered edge linking `swift:…::AppClient` to `objc:…::AppClient` with selector
`doWork:`, a cross-language relation may cite the edge. Header co-location without
registration yields `apple_bridge_edge_missing`.

### Negative — target membership is not reach

Given `FooTests` listing `Foo.swift` only because it shares a target, no test oracle
may credit `Foo.swift` owners unless a registered test method statically references the
owner with bound sink.

### Positive — Swift overload pin

Given two `func compute(_ x: Int)` and `func compute(_ x: String)` and a change in the
Int overload body, probes attach to the Int signature digest; the String overload remains
unrelated.

### Negative — protocol witness unresolved

Given a protocol extension default implementation and a conforming type without explicit
witness provenance, changed behavior in the default issues `swift_witness_unresolved`
rather than `exposed`.

### Positive — Objective-C category replacement

Given a category `- (void)send` on `Client` replacing class behavior, owner id includes
category name and implementation span; static dispatch assumptions fail with
`objc_category_replacement_unresolved` when replacement cannot be resolved.

### Negative — dynamic selector

Given `performSelector:` or `objc_msgSend` without static selector target, emit
`objc_selector_dispatch_unresolved`; no owner reach through name similarity.

### Positive — Swift Testing parameterized row

Given `@Test(arguments: [1, 2, 3])` and `#expect(f(x) == g(x))` binding the changed sink,
registration state is `parameterized_generated` until row binding is static; eligible rows
with bound sink may reach strong exact.

### Negative — known issue before oracle

Given `withKnownIssue { … }` wrapping a failing `#expect` on the changed sink, state is
`known_issue`; oracle eligibility follows #5542 (not treated as ordinary failure).

### Positive — Swift async expectation

Given an async test awaiting a changed `async` function and `XCTAssertEqual` on the result
after `await`, credit requires `async_expectation_bound` plus established activation.

### Negative — unavailable platform

Given `@available(iOS 18, *)` guarding changed code and analysis context pinned to iOS 17
SDK facts, emit `apple_sdk_availability_unresolved`; do not universalize the claim.

### Negative — Objective-C++ unpromoted

Given changed logic in a `.mm` file using C++ templates, emit `apple_objcpp_mode_unpromoted`
unless the Objective-C++ cell is explicitly promoted.

### Negative — UI test boundary

Given a UI test touching a view identifier near production code, classify `ui_only`; no
static production owner reach from UI harness alone.

## Test Mapping

Planned mappings (implementation slices):

| Slice | Proof owner |
| --- | --- |
| #5685 SwiftPM/Xcode packet parse | `fixtures/apple-project-facts-v1/` |
| #5686 Swift owners/probes | `fixtures/apple-swift-verdict-corpus/` |
| #5687 Objective-C owners/probes | `fixtures/apple-objc-verdict-corpus/` |
| #5688 Swift Testing oracles | framework corpus + spec-scope rows |
| #5689 Swift XCTest oracles | framework corpus + spec-scope rows |
| #5690 Objective-C XCTest oracles | framework corpus + spec-scope rows |
| #5691 cross-language relations | `fixtures/cross-language-edge-registry-corpus/` extensions |
| #5694 judged corpora | `metrics/apple-verdict-corpus/` |

Spec PR mapping:

- `xtask/tests/spec_format_determinism.rs::spec_format_does_not_require_git_history`
- `cargo xtask check-spec-format`, `check-spec-numbering`, `check-traceability`,
  `check-doc-artifacts`, `check-doc-index`

## Implementation Mapping

| Phase | Issue | Delivers |
| --- | --- | --- |
| Spec | #5682 | This document + indexes |
| ADR | #5683 | Producer substrate choice |
| Scaffold | #5684 | `LanguageId`, routing, Appendix B cells in registry |
| Project facts | #5685 | `apple-project-facts-v1` producer |
| Swift owners | #5686 | Swift adapter owners/probes |
| ObjC owners | #5687 | Objective-C adapter owners/probes |
| Framework oracles | #5688–#5690 | Separate test/oracle producers |
| Relations | #5691 | Bridge/async/runtime limits |
| Repo mode | #5692 | Multi-platform completeness |
| Output | #5693 | Shared surfaces |
| Eval | #5694 | Judged corpora and support adjudication |

## Metrics

- `spec_contract_registered` for this PR.
- Later: packet parse success, limitation distribution, false bridge rate, framework
  oracle precision, corpus adjudication pass rates (#5694).

---

## Appendix A — Apple `static_limit_kind` registry (v1)

Closed enum values producers must use (extend only via spec amendment):

```text
apple_package_manifest_unexecuted
apple_xcode_build_not_run
apple_project_fact_partial
swift_overload_unresolved
swift_generic_constraint_unresolved
swift_opaque_type_unresolved
swift_existential_dispatch_unresolved
swift_witness_unresolved
swift_extension_dispatch_unresolved
swift_actor_isolation_unresolved
swift_async_scheduling_unresolved
swift_macro_expansion_unresolved
swift_result_builder_unresolved
swift_reflection_unresolved
objc_selector_dispatch_unresolved
objc_category_replacement_unresolved
objc_dynamic_messaging_unresolved
objc_forwarding_unresolved
objc_swizzling_detected
objc_runtime_lookup_unresolved
apple_preprocessor_context_unresolved
apple_sdk_availability_unresolved
apple_bridge_edge_missing
apple_generated_interface_unresolved
apple_test_registration_unestablished
apple_test_activation_unestablished
apple_oracle_framework_mismatch
apple_objcpp_mode_unpromoted
apple_ui_test_unpromoted
apple_performance_test_unpromoted
```

## Appendix B — Planned capability cells (`apple-capability-cells-v1`)

Machine-readable registry for #5531. All cells start `not_proven` / `preview`.
`proof_refs = ["RIPR-SPEC-0395"]`. Implementation copies into
`domain/language_capability.rs` in #5684 without semantic widening.

Registry identity law: each `id` MUST match the four-axis
`{language}/{framework}/{mode}/{family}` shape consumed by
`domain/language_capability.rs` and the manifest join
([RIPR-SPEC-0365](RIPR-SPEC-0365-capability-manifest-cell-join.md)). Platform-specific
dispositions use framework values such as `platform-macos` rather than a fifth axis.
Every `not_proven` cell names a `limit` string because the registry validator requires it.

```toml
schema_version = "apple-capability-cells-v1"
spec_id = "RIPR-SPEC-0395"
default_limit = "Normative contract only; Apple adapters and judged corpora not landed (#5682)"

[[cell]]
id = "swift/any/diff/static_facts"
language = "swift"
framework = "any"
mode = "diff"
family = "static_facts"
support_state = "not_proven"
tier_ceiling = "preview"
limit = "Normative contract only; Apple adapters and judged corpora not landed (#5682)"
proof_refs = ["RIPR-SPEC-0395"]

[[cell]]
id = "swift/any/repo/static_facts"
language = "swift"
framework = "any"
mode = "repo"
family = "static_facts"
support_state = "not_proven"
tier_ceiling = "preview"
limit = "Normative contract only; Apple adapters and judged corpora not landed (#5682)"
proof_refs = ["RIPR-SPEC-0395"]

[[cell]]
id = "swift/swift-testing/diff/static_facts"
language = "swift"
framework = "swift-testing"
mode = "diff"
family = "static_facts"
support_state = "not_proven"
tier_ceiling = "preview"
limit = "Normative contract only; Apple adapters and judged corpora not landed (#5682)"
proof_refs = ["RIPR-SPEC-0395"]

[[cell]]
id = "swift/xctest/diff/static_facts"
language = "swift"
framework = "xctest"
mode = "diff"
family = "static_facts"
support_state = "not_proven"
tier_ceiling = "preview"
limit = "Normative contract only; Apple adapters and judged corpora not landed (#5682)"
proof_refs = ["RIPR-SPEC-0395"]

[[cell]]
id = "objective_c/any/diff/static_facts"
language = "objective_c"
framework = "any"
mode = "diff"
family = "static_facts"
support_state = "not_proven"
tier_ceiling = "preview"
limit = "Normative contract only; Apple adapters and judged corpora not landed (#5682)"
proof_refs = ["RIPR-SPEC-0395"]

[[cell]]
id = "objective_c/any/repo/static_facts"
language = "objective_c"
framework = "any"
mode = "repo"
family = "static_facts"
support_state = "not_proven"
tier_ceiling = "preview"
limit = "Normative contract only; Apple adapters and judged corpora not landed (#5682)"
proof_refs = ["RIPR-SPEC-0395"]

[[cell]]
id = "objective_c/xctest/diff/static_facts"
language = "objective_c"
framework = "xctest"
mode = "diff"
family = "static_facts"
support_state = "not_proven"
tier_ceiling = "preview"
limit = "Normative contract only; Apple adapters and judged corpora not landed (#5682)"
proof_refs = ["RIPR-SPEC-0395"]

[[cell]]
id = "apple-project/any/diff/static_facts"
language = "apple-project"
framework = "any"
mode = "diff"
family = "static_facts"
support_state = "not_proven"
tier_ceiling = "preview"
limit = "Shared SwiftPM/Xcode substrate facts only; no language owner semantics (#5682)"
proof_refs = ["RIPR-SPEC-0395"]

[[cell]]
id = "apple-project/any/repo/static_facts"
language = "apple-project"
framework = "any"
mode = "repo"
family = "static_facts"
support_state = "not_proven"
tier_ceiling = "preview"
limit = "Shared SwiftPM/Xcode substrate facts only; no language owner semantics (#5682)"
proof_refs = ["RIPR-SPEC-0395"]

# Explicit unpromoted modes (must remain separate decisions)

[[cell]]
id = "objective_c/objcpp/diff/static_facts"
language = "objective_c"
framework = "objcpp"
mode = "diff"
family = "static_facts"
support_state = "not_proven"
tier_ceiling = "preview"
limit = "Objective-C++ is not implied by .m support"
proof_refs = ["RIPR-SPEC-0395"]

[[cell]]
id = "swift/xctest-ui/diff/static_facts"
language = "swift"
framework = "xctest-ui"
mode = "diff"
family = "static_facts"
support_state = "not_proven"
tier_ceiling = "preview"
limit = "UI tests are not production seam oracles by default"
proof_refs = ["RIPR-SPEC-0395"]

[[cell]]
id = "objective_c/xctest-ui/diff/static_facts"
language = "objective_c"
framework = "xctest-ui"
mode = "diff"
family = "static_facts"
support_state = "not_proven"
tier_ceiling = "preview"
limit = "UI tests are not production seam oracles by default"
proof_refs = ["RIPR-SPEC-0395"]

# Platform-scoped rows (configuration context must not universalize)

[[cell]]
id = "swift/platform-macos/diff/static_facts"
language = "swift"
framework = "platform-macos"
mode = "diff"
family = "static_facts"
support_state = "not_proven"
tier_ceiling = "preview"
limit = "macOS SDK/deployment context is scoped separately from generic Swift facts"
proof_refs = ["RIPR-SPEC-0395"]

[[cell]]
id = "swift/platform-ios/diff/static_facts"
language = "swift"
framework = "platform-ios"
mode = "diff"
family = "static_facts"
support_state = "not_proven"
tier_ceiling = "preview"
limit = "iOS SDK/deployment context is scoped separately from generic Swift facts"
proof_refs = ["RIPR-SPEC-0395"]

[[cell]]
id = "swift/platform-tvos/diff/static_facts"
language = "swift"
framework = "platform-tvos"
mode = "diff"
family = "static_facts"
support_state = "not_proven"
tier_ceiling = "preview"
limit = "tvOS SDK/deployment context is scoped separately from generic Swift facts"
proof_refs = ["RIPR-SPEC-0395"]

[[cell]]
id = "swift/platform-watchos/diff/static_facts"
language = "swift"
framework = "platform-watchos"
mode = "diff"
family = "static_facts"
support_state = "not_proven"
tier_ceiling = "preview"
limit = "watchOS SDK/deployment context is scoped separately from generic Swift facts"
proof_refs = ["RIPR-SPEC-0395"]

[[cell]]
id = "swift/platform-visionos/diff/static_facts"
language = "swift"
framework = "platform-visionos"
mode = "diff"
family = "static_facts"
support_state = "not_proven"
tier_ceiling = "preview"
limit = "visionOS SDK/deployment context is scoped separately from generic Swift facts"
proof_refs = ["RIPR-SPEC-0395"]

[[cell]]
id = "objective_c/platform-macos/diff/static_facts"
language = "objective_c"
framework = "platform-macos"
mode = "diff"
family = "static_facts"
support_state = "not_proven"
tier_ceiling = "preview"
limit = "macOS SDK/deployment context is scoped separately from generic Objective-C facts"
proof_refs = ["RIPR-SPEC-0395"]

[[cell]]
id = "objective_c/platform-ios/diff/static_facts"
language = "objective_c"
framework = "platform-ios"
mode = "diff"
family = "static_facts"
support_state = "not_proven"
tier_ceiling = "preview"
limit = "iOS SDK/deployment context is scoped separately from generic Objective-C facts"
proof_refs = ["RIPR-SPEC-0395"]

[[cell]]
id = "objective_c/platform-tvos/diff/static_facts"
language = "objective_c"
framework = "platform-tvos"
mode = "diff"
family = "static_facts"
support_state = "not_proven"
tier_ceiling = "preview"
limit = "tvOS SDK/deployment context is scoped separately from generic Objective-C facts"
proof_refs = ["RIPR-SPEC-0395"]

[[cell]]
id = "objective_c/platform-watchos/diff/static_facts"
language = "objective_c"
framework = "platform-watchos"
mode = "diff"
family = "static_facts"
support_state = "not_proven"
tier_ceiling = "preview"
limit = "watchOS SDK/deployment context is scoped separately from generic Objective-C facts"
proof_refs = ["RIPR-SPEC-0395"]

[[cell]]
id = "objective_c/platform-visionos/diff/static_facts"
language = "objective_c"
framework = "platform-visionos"
mode = "diff"
family = "static_facts"
support_state = "not_proven"
tier_ceiling = "preview"
limit = "visionOS SDK/deployment context is scoped separately from generic Objective-C facts"
proof_refs = ["RIPR-SPEC-0395"]

# Repair routing remains unavailable until explicit follow-on promotion

[[cell]]
id = "swift/any/diff/repair_routing"
language = "swift"
framework = "any"
mode = "diff"
family = "repair_routing"
support_state = "not_proven"
tier_ceiling = "preview"
limit = "Apple repair routing is not implemented or promoted (#5682)"
proof_refs = ["RIPR-SPEC-0395"]

[[cell]]
id = "objective_c/any/diff/repair_routing"
language = "objective_c"
framework = "any"
mode = "diff"
family = "repair_routing"
support_state = "not_proven"
tier_ceiling = "preview"
limit = "Apple repair routing is not implemented or promoted (#5682)"
proof_refs = ["RIPR-SPEC-0395"]
```
