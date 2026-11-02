# RIPR-SPEC-0399: Native Family Evidence Contract

Status: proposed

Owner: analysis / native family

Created: 2026-11-02

Linked issues:

- [#5696](https://github.com/EffortlessMetrics/ripr-swarm/issues/5696) — normative
  translation-unit, configuration, C, C++, test, UB, and FFI evidence contracts
- [#5599](https://github.com/EffortlessMetrics/ripr-swarm/issues/5599) — native
  epic parent
- [#5582](https://github.com/EffortlessMetrics/ripr-swarm/issues/5582) — language
  expansion programme

Consumed architecture (by reference; not implemented in this slice):

- [#5526](https://github.com/EffortlessMetrics/ripr-swarm/issues/5526) — shared
  multi-language architecture
- [#5531](https://github.com/EffortlessMetrics/ripr-swarm/issues/5531) — capability
  cell registry semantics
- [#5532](https://github.com/EffortlessMetrics/ripr-swarm/issues/5532) — preview
  posture and authority boundaries
- [#5541](https://github.com/EffortlessMetrics/ripr-swarm/issues/5541) — owner and
  probe identity law
- [#5542](https://github.com/EffortlessMetrics/ripr-swarm/issues/5542) — test
  registration, activation, and oracle eligibility
- [#5546](https://github.com/EffortlessMetrics/ripr-swarm/issues/5546) — exact-span
  probe and source-map provenance
- [#5559](https://github.com/EffortlessMetrics/ripr-swarm/issues/5559) — cross-language
  C ABI / FFI / generated-binding edges
- [#5560](https://github.com/EffortlessMetrics/ripr-swarm/issues/5560) — generated,
  vendor, and external source roles

Linked specs:

- [RIPR-SPEC-0026](RIPR-SPEC-0026-language-adapter-contract.md) — language-neutral
  adapter boundary
- [RIPR-SPEC-0135](RIPR-SPEC-0135-assurance-state-vocabulary.md) — static vs
  executed assurance axes
- [RIPR-SPEC-0224](RIPR-SPEC-0224-examined-test-miss-evidence.md) — related-test
  miss vocabulary
- [RIPR-SPEC-0378](RIPR-SPEC-0378-cross-language-edge-registry-v1.md) — registered
  FFI edge schema

Support-tier impact:

- See: [support tiers](../status/SUPPORT_TIERS.md).
- No support-tier promotion. Initial C and C++ support remains preview/advisory per
  language/framework/configuration/mode. This document defines contracts only;
  it does not enable adapters, promote support tiers, or change gates or badges.

## Problem

The native family (C and C++) shares compilation databases, include graphs, build
models, and Clang-family tooling facts, but C and C++ are not semantically
interchangeable. Without a normative contract, a shared substrate could flatten
overload resolution, templates, virtual dispatch, and C-specific UB-sensitive
patterns into one undifferentiated analyzer, or could treat an include edge, target
name, or macro spelling as proof of test reach.

This spec defines the reusable build/translation-unit/configuration fact substrate
and separate C and C++ owner, activation, oracle, relation, limitation, and support
claims. Implementation slices (#5697 onward) must conform to these laws; they must
not invent parallel vocabulary.

## Behavior

### Posture and authority

- C and C++ adapters are independent semantic rows. A shared compiler family or
  `compile_commands.json` entry does not merge their owner kinds, probe shapes, or
  oracle bindings.
- Every emitted finding for C or C++ carries `language` (`c` or `cpp`),
  `language_status = preview`, and a mandatory `translation_unit_context_id` (see
  below). Positive reach, propagation, or oracle claims without that context are
  invalid contract output.
- Capability cells listed in [Capability cell registry](#capability-cell-registry)
  are registered for traceability and future manifest joins. All native cells remain
  `not_proven` until a later adjudication slice (#5709); this spec does not add
  manifest entries or default-enable any language.

### Shared translation-unit and configuration substrate

The substrate is language-neutral. It records candidate and admitted compile
contexts without executing configure, generate, build, link, test, sanitizer, or
mutation commands.

#### Identities

```text
NativeBuildProjectIdentityV1
  project_root: normalized repository-relative path
  build_system: cmake | meson | compile_commands_only | unknown
  generator: optional string (e.g. Ninja, Unix Makefiles)
  selected_configuration: optional string (Debug, Release, …)
  unresolved_configurations: ordered list of strings still competing

NativeCompileContextV1
  context_id: stable hash of canonical fields below
  working_directory: normalized path
  compiler_family: clang | gcc | msvc | apple_clang | other | unknown
  compiler_version: optional string
  target_triple: optional string
  language_mode: c | cpp | objective_c | objective_cpp | unknown
  language_standard: optional string (c17, c++20, gnu++17, …)
  command_argv: ordered argv of the compile command (no shell interpolation)
  defines: map name → optional value (undefines recorded as absent keys with
    explicit undef list)
  undefines: ordered macro names
  include_paths: ordered IncludePathV1 (quote, angle, system, framework)
  forced_includes: ordered paths
  sysroot: optional path
  response_files: ordered paths expanded only when explicitly parsed
  pch_or_module_inputs: optional structured refs (PCH, C++20 module, header unit)
  sanitizer_flags: ordered tokens when present on the command line
  optimization_flags: ordered tokens when semantically relevant to UB/evidence law

TranslationUnitFactV1
  tu_id: stable hash of primary_source path + context_id
  primary_source: normalized path of the TU primary file
  compile_context: NativeCompileContextV1
  source_role: production | test | benchmark | generated | vendor | external |
    system | build_output | unknown
  build_target: optional target name from build model (candidate only until bound)
  direct_includes: ordered IncludeDirectiveFactV1 (path token, quote vs angle,
    macro-expanded include remains macro-mapped)
  module_or_header_unit: optional ModuleHeaderUnitFactV1
  linkage_candidates: optional list (object, archive, shared lib) — co-membership
    never establishes executable reach
  physical_file_sha256: optional content digest when file was read
  authored_to_expanded_map: optional span map (#5546) from physical file to
    macro-expanded or generated views
```

#### Currentness and multiplicity

- The same physical `.h` or `.c` file under two distinct `NativeCompileContextV1`
  values is two translation units (`tu_id` differs). Relations, probes, and oracles
  from one context do not transfer to another.
- A **candidate** compile context discovered from CMake/Meson/compilation-database
  scans is not an **admitted** context until preprocessor branches, language mode,
  and primary source binding are resolved (#5700). Positive claims require admission.
- Include graph edges, declaration-only headers, target dependencies, library names,
  object/archive membership, linker inputs, or symbol name coincidence do not
  establish test reach or production activation.

#### Cross-language FFI edges

Registered C ABI / FFI / generated-binding relations use
[RIPR-SPEC-0378](RIPR-SPEC-0378-cross-language-edge-registry-v1.md) with
`BoundaryFamilyV1` values appropriate to native boundaries (`ffi_binding`,
`generated_binding`, `native_addon`, …). An unregistered foreign call site remains
a typed limitation, not a silent relation.

### C semantic contract

C owners and probes are separate from C++ kinds. Stable owner identifiers are
language-qualified, TU-qualified, and path-qualified:

```text
c:<tu_id>::<owner_kind>:<stable_local_id>
```

#### C owner kinds (closed)

| Kind | Covers |
| --- | --- |
| `c_function` | Functions and static functions; internal/external linkage |
| `c_inline_function` | Inline function definitions |
| `c_declaration` | Declarations without a selected definition in this TU/context |
| `c_global` | File-scope and static globals, thread-local storage |
| `c_typedef` | Typedef names |
| `c_tagged_type` | struct/union/enum tags and named aggregates |
| `c_field` | Struct/union fields |
| `c_enum_constant` | Enumerators |
| `c_label` | Labels where admitted |
| `c_macro` | Object- and function-like macros as source-mapped names (expansion mapped, not evaluated) |
| `c_function_pointer` | Function pointer objects and callback slots |
| `c_compound_literal` | Compound literals where admitted |

#### C probe families (exact-span)

Probes attach to exact spans in the admitted TU map (#5546). Non-exhaustive closed
probe intents:

- condition and switch-case arms
- return and status/error-code expressions
- pointer, null, and bounds-sensitive operations
- global/state mutation and resource ownership effects
- calls, indirect calls, and callback invocations
- output and side-effect sinks

C-specific limits (must emit `static_limit_kind`, never coerce to `no_static_path`):

| `static_limit_kind` | When |
| --- | --- |
| `native_compile_command_missing` | Primary source lacks an admitted compile context |
| `native_preprocessor_branch_unresolved` | Active `#if` branch unknown |
| `native_macro_expansion_unresolved` | Probe span inside unmapped macro expansion |
| `c_function_pointer_unresolved` | Indirect call without resolvable target set |
| `c_variadic_call_unresolved` | Variadic actuals not structurally bound |
| `c_volatile_atomic_signal_unresolved` | volatile/atomic/signal interaction |
| `c_setjmp_longjmp_unresolved` | Non-local control flow |
| `c_lifetime_alias_ub_unresolved` | Alias, lifetime, or UB-sensitive access pattern |
| `c_declaration_only` | Behavior selected only in another TU/configuration |
| `c_implementation_defined_unresolved` | Implementation-defined behavior without configuration fact |

Undefined-behavior **possibility** is recorded independently from test oracle
adequacy (see [UB and runtime boundary](#ub-and-runtime-boundary)).

### C++ semantic contract

C++ owners are not aliases of C owners. Identifiers:

```text
cpp:<tu_id>::<owner_kind>:<stable_local_id>
```

#### C++ owner kinds (closed)

| Kind | Covers |
| --- | --- |
| `cpp_namespace` | Namespace scopes |
| `cpp_type` | class/struct/union/enum types |
| `cpp_function` | Free functions, static members |
| `cpp_method` | Non-static methods (virtual flag recorded) |
| `cpp_constructor` / `cpp_destructor` | Special members |
| `cpp_operator` | Operator and conversion functions |
| `cpp_field` | Data members |
| `cpp_lambda` | Closure types and call operators |
| `cpp_template` | Templates, specializations, instantiations |
| `cpp_concept` | Named constraints where visible |
| `cpp_module_interface` | Module partitions/interfaces |
| `cpp_coroutine` | Coroutine promises where visible |

#### C++ probe families (exact-span)

- branches and pattern-like constructs (`if`, `switch`, `co_await`, …)
- `return`, `co_return`, `yield` values
- exception throw, catch, and RAII guard boundaries
- lifetime, state, and resource effects
- construction, calls, overload-resolved calls, virtual calls
- boundary conversions and casts

C++-specific `static_limit_kind` values:

| `static_limit_kind` | When |
| --- | --- |
| `cpp_overload_unresolved` | Overload set not resolved in this context |
| `cpp_template_instantiation_unresolved` | Dependent body not instantiated |
| `cpp_adl_unresolved` | Argument-dependent lookup ambiguous |
| `cpp_virtual_dispatch_unresolved` | Dynamic dispatch target set unresolved |
| `cpp_implicit_conversion_unresolved` | Conversion path ambiguous |
| `cpp_exception_flow_unresolved` | Exception path not structurally closed |
| `cpp_coroutine_flow_unresolved` | Coroutine suspend/resume not modeled |
| `cpp_module_import_unresolved` | Module/header-unit edge not admitted |
| `cpp_type_erasure_unresolved` | Erased call target unknown |

### Test registration and oracle contract

Test facts obey [#5542](https://github.com/EffortlessMetrics/ripr-swarm/issues/5542).
Registration precedes oracle eligibility: a macro spelling or test name alone is not
an oracle.

#### Registration states (orthogonal)

```text
registered | selected | active | disabled | skipped | fixture_bound |
parameterized | typed | value_generated | section_path | generator_path |
death_isolated | platform_limited | configuration_limited |
expected_failure_like | unknown
```

#### First-scope frameworks (separate rows)

| Language | Framework | Registration facts | Oracle bindings |
| --- | --- | --- | --- |
| C | CMocka | suite/group/case, setup/teardown, `will_return`/`expect_*` | `assert_*`, mock expectations with bound callee |
| C | Unity | `RUN_TEST`, fixtures, groups | `TEST_ASSERT_*` macros with resolved headers |
| C++ | GoogleTest | `TEST`, `TEST_F`, typed/parameterized templates | `EXPECT_*`, `ASSERT_*` with bound actual |
| C++ | GoogleMock | mock objects, `EXPECT_CALL`/`ON_CALL` | interaction oracles tied to mocked signature |
| C++ | Catch2 | `TEST_CASE`, `SECTION`, `GENERATE` | `REQUIRE`, `CHECK`, matchers via registered binding |
| C++ | doctest | `TEST_CASE`, `SUBCASE` | `CHECK`, `REQUIRE` |

Oracle credit requires:

1. admitted TU context for the test translation unit;
2. framework identity established via resolved include/module facts for that context;
3. exact-span match between assertion/matcher/macro expansion and a bound observable
   sink (#5546);
4. related production owner in the **same or registered cross-TU context** only when
   a positive edge exists in the relation layer (#5706).

Custom matchers, generator expressions, death-test subprocesses, mock
interposition, and section/generator paths that are not structurally resolved emit
framework-specific limitations (`native_test_custom_matcher_unresolved`,
`native_test_death_process_unresolved`, `native_test_mock_interposition_unresolved`,
…).

### UB and runtime boundary

Independent fact axes (must not collapse):

```text
activation
reach / propagation
observation / discrimination
undefined_behavior_possibility
sanitizer_witness (external)
mutation_witness (external)
```

- A test may discriminate a defined observable while UB remains possible in the
  same owner; static output must not describe UB possibility as “missing an
  assertion.”
- Sanitizer or mutation results are recorded only through
  [RIPR-SPEC-0135](RIPR-SPEC-0135-assurance-state-vocabulary.md) external axes.
  Static syntax cannot infer sanitizer or mutation outcomes.
- `undefined_behavior_possibility` is a typed static limitation or advisory flag,
  not an exposure class promotion.

### Limitation vocabulary (native family)

Unknown reach uses typed native limits. **`no_static_path` is forbidden** when the
stop reason is missing compile context, unresolved preprocessor/platform/compiler
configuration, unresolved overload/template/virtual/callback semantics, or
framework/custom-oracle indirection.

Additional shared stops:

| `static_limit_kind` | When |
| --- | --- |
| `native_platform_architecture_unresolved` | Selected CPU/OS/endianness unknown |
| `native_compiler_standard_unresolved` | Language standard not fixed for context |
| `native_generated_vendor_external` | Generated/vendor/external/build-output role blocks repair targeting |
| `native_ffi_boundary_unregistered` | Foreign call without registered edge |
| `native_assembly_intrinsic_builtin` | Assembly/intrinsic/builtin boundary |
| `native_configuration_alternatives_unresolved` | Multiple admitted contexts conflict |

### Capability cell registry

The following cells are registered by this spec for documentation, traceability, and
future manifest joins. Support state remains `not_proven` until #5709.

| Cell ID | Semantics |
| --- | --- |
| `c/any/diff/static_facts` | C preview static facts in diff mode |
| `c/any/repo/static_facts` | C preview static facts in repo mode |
| `cpp/any/diff/static_facts` | C++ preview static facts in diff mode |
| `cpp/any/repo/static_facts` | C++ preview static facts in repo mode |
| `c/cmocka/diff/test_oracle` | CMocka registration and oracle binding |
| `c/cmocka/repo/test_oracle` | CMocka registration and oracle binding |
| `c/unity/diff/test_oracle` | Unity registration and oracle binding |
| `c/unity/repo/test_oracle` | Unity registration and oracle binding |
| `cpp/googletest/diff/test_oracle` | GoogleTest registration and oracle binding |
| `cpp/googletest/repo/test_oracle` | GoogleTest registration and oracle binding |
| `cpp/googlemock/diff/test_oracle` | GoogleMock interaction oracles |
| `cpp/googlemock/repo/test_oracle` | GoogleMock interaction oracles |
| `cpp/catch2/diff/test_oracle` | Catch2 registration/oracle/sections |
| `cpp/catch2/repo/test_oracle` | Catch2 registration/oracle/sections |
| `cpp/doctest/diff/test_oracle` | doctest registration/oracle/subcases |
| `cpp/doctest/repo/test_oracle` | doctest registration/oracle/subcases |
| `native/any/diff/translation_unit_substrate` | Shared TU/configuration substrate |
| `native/any/repo/translation_unit_substrate` | Shared TU/configuration substrate |

Configuration-specific rows (e.g. `Debug` vs `Release`) are modeled as separate
admitted `context_id` values, not as alternate cell IDs, until a later slice proves
otherwise.

## Required Evidence

This slice is spec-only. Conformance evidence for #5696 is:

- This document’s machine-readable tables and schema blocks.
- Repository registration in `docs/spec-index.d/`, `policy/doc-artifacts.d/`,
  `.ripr/traceability.d/`, and verdict-corpus `spec-scopes/` fragments.
- `cargo xtask check-spec-format`, `check-spec-numbering`, `check-doc-artifacts`,
  and `check-traceability` on the registering PR.

Future implementation slices add producer tests and
`fixtures/native-evidence-contract-corpus/` keyed to the acceptance examples below.

## Non-Goals

- No compiler invocation, configure/generate/build/link, test execution, sanitizer,
  debugger, fuzzer, mutation runner, or plugin execution during passive analysis.
- No adapter, parser, routing, or Rust implementation in the #5696 slice.
- No default language enablement, gate promotion, badge, baseline, RIPR Zero, package
  publication, or support-tier promotion.
- No proof of absence of UB, memory safety, or complete template/runtime dispatch.
- No collapsing C and C++ semantics or treating shared tooling as shared owners.

## Acceptance Examples

Each example names expected contract disposition. “Must not claim” means no positive
reach/oracle/exposed classification.

### Shared translation unit and configuration

1. **Positive — dual context:** `header.h` included from `a.c` (C17, `-DFEATURE=1`)
   and `b.c` (C17, `-DFEATURE=0`) yields two `TranslationUnitFactV1` rows with
   different `context_id` and `tu_id`. A probe in an inactive `#if` branch emits
   `native_preprocessor_branch_unresolved`.
2. **Negative — include reach:** `tests/t.c` includes `prod.h` but no registered test
   relation or oracle binding exists. Must not claim production activation from the
   include alone.
3. **Negative — target co-membership:** `libfoo.a` lists both `prod.o` and `test.o`
   as members. Must not infer test reach between object files without registration
   facts.

### C callbacks and UB

4. **Positive — resolved callback:** Function pointer `cb` assigned only
   `&helper` in admitted context; indirect call through `cb` may bind probe reach to
   `helper` with span provenance.
5. **Negative — unresolved callback:** `cb` loaded from external struct; emit
   `c_function_pointer_unresolved`; must not emit `no_static_path` as a substitute.
6. **UB vs oracle — buffer access:** Test asserts return code `0` while static facts
   record `c_lifetime_alias_ub_unresolved` on an out-of-bounds store. Oracle and UB
   limitation must both appear; static output must not say the gap is only “missing
   an assertion.”

### C++ overloads, templates, virtuals

7. **Positive — resolved overload:** Call `f(1)` binds to `f(int)` under admitted
   context; probe owner is the selected overload’s body span.
8. **Negative — ambiguous overload:** `f("")` matches multiple templates; emit
   `cpp_overload_unresolved` or `cpp_template_instantiation_unresolved`.
9. **Negative — virtual dispatch:** Call through base pointer with incomplete vtable
   facts; emit `cpp_virtual_dispatch_unresolved`; must not credit a single override
   without proof.

### Test frameworks (registration before oracle)

10. **CMocka positive:** `assert_int_equal(out, 3)` in a TU with admitted CMocka
    headers and registered case binds oracle to observable `out` at exact span.
11. **Unity negative — bare macro:** Source contains `TEST_ASSERT_EQUAL(x, y)` but
    Unity headers are not resolved in this compile context. No oracle credit.
12. **GoogleMock positive:** `EXPECT_CALL(mock, Foo(42)).Times(1)` with registered
    mock type and admitted context binds interaction oracle; production reach through
    mock interposition may be defeated per #5599 acceptance.
13. **Catch2 section path:** Assertion in `SECTION("edge")` carries section-path
    registration; unrelated section must not inherit oracle.
14. **Death test isolation:** `ASSERT_DEATH` records `death_isolated` registration;
    static observation does not claim subprocess outcome.

### FFI

15. **Positive — registered edge:** Rust `extern "C"` fn declared in registry with
    `ffi_binding` edge to C definition; relation cites edge id and both TU contexts.
16. **Negative — unregistered foreign call:** Call to `foreign_symbol` without registry
    edge; emit `native_ffi_boundary_unregistered`.

## Test Mapping

Planned implementation tests (out of scope for #5696):

- `fixtures/native-evidence-contract-corpus/corpus.json` — one row per acceptance
  example above.
- Future native adapter unit tests under `crates/ripr/src/analysis/language/native/`.

Registering PR evidence:

- `cargo xtask check-spec-format`
- `cargo xtask check-spec-numbering`
- `cargo xtask check-doc-artifacts`
- `cargo xtask check-traceability`

## Implementation Mapping

Planned (not part of this spec slice):

- `crates/ripr/src/analysis/language/native/` — shared substrate and routers (#5698–#5700)
- `crates/ripr/src/analysis/language/c/` — C owners/probes (#5701, #5703)
- `crates/ripr/src/analysis/language/cpp/` — C++ owners/probes (#5702, #5704, #5705)
- `crates/ripr/src/provider_contract/` — TU DTO serialization alongside #5697 ADR

## Metrics

- `spec_format_pass_rate`
- `spec_index_registration_complete`
- `traceability_behavior_registered`
