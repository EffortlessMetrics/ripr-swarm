# RIPR-SPEC-0405: Julia preview package, multiple-dispatch, Test, numerical, array, and native boundaries

Status: proposed

Owner: language-adapter / ecosystem (#5593)

Created: 2026-11-02

Linked issues:

- [#5862](https://github.com/EffortlessMetrics/ripr-swarm/issues/5862) (this contract)
- Parent epic [#5593](https://github.com/EffortlessMetrics/ripr-swarm/issues/5593)
- Programme [#5582](https://github.com/EffortlessMetrics/ripr-swarm/issues/5582)
- Shared architecture [#5526](https://github.com/EffortlessMetrics/ripr-swarm/issues/5526)
- Consumed authorities: [#5531](https://github.com/EffortlessMetrics/ripr-swarm/issues/5531), [#5532](https://github.com/EffortlessMetrics/ripr-swarm/issues/5532), [#5541](https://github.com/EffortlessMetrics/ripr-swarm/issues/5541), [#5542](https://github.com/EffortlessMetrics/ripr-swarm/issues/5542), [#5546](https://github.com/EffortlessMetrics/ripr-swarm/issues/5546), [#5559](https://github.com/EffortlessMetrics/ripr-swarm/issues/5559), [#5560](https://github.com/EffortlessMetrics/ripr-swarm/issues/5560)

Linked specs:

- [RIPR-SPEC-0026](RIPR-SPEC-0026-language-adapter-contract.md) (language-neutral adapter boundary)
- [RIPR-SPEC-0030](RIPR-SPEC-0030-preview-evidence-policy-boundary.md) (preview posture)
- [RIPR-SPEC-0378](RIPR-SPEC-0378-cross-language-edge-registry-v1.md) (registered cross-language edges)

Support-tier impact:

- This spec does not enable Julia, promote support tiers, arm default gates,
  badges, baselines, RIPR Zero blocking, or publication. All Julia evidence
  governed here remains preview/advisory until separate promotion work cites
  proof. The canonical support-tier boundary remains
  [support tiers](../status/SUPPORT_TIERS.md).

Policy impact:

- Register in `policy/doc-artifacts.d/RIPR-SPEC-0405.toml`.
- Add `docs/spec-index.d/RIPR-SPEC-0405.md`.
- Register `.ripr/traceability.d/` behavior `RIPR-SPEC-0405`.
- Register `fixtures/*/verdict-corpus/spec-scopes/RIPR-SPEC-0405.toml` as out of
  scope for runtime mutant corpora until Julia judged corpora land.
- No adapter, parser, routing, feature gate, or `LanguageId` registration is
  introduced by this document alone.

## Problem

Julia package projects combine multiple dispatch, generic numerical code,
macros, generated functions, scientific arrays, tolerance-based tests, native
`ccall`, optional GPU kernels, and package extensions. Without a normative
preview contract, passive analysis could collapse every method under a generic
function name, treat `Project.toml` dependency strings as reach, merge array
shape with element values, or promote approximate `@test` credit without
tolerance and observed-quantity evidence.

Epic #5593 needs one documentation-only law for wire identity, package roles,
method-signature identity, standard `Test` activation, numerical and array
observations, and native/GPU edges before parser selection, routing, or adapters
ship.

## Behavior

Initial Julia support is preview and advisory only. The spec grants no gate,
badge, baseline, RIPR Zero, default-feature, repair-loop, or publication
authority.

The future Julia preview adapter is enabled by repo configuration:

```toml
[languages]
enabled = ["rust", "julia"]
```

and the `lang-julia` Cargo feature when compiled. It routes at least:

- `*.jl` source files;
- `Project.toml`, bounded `Manifest.toml` context, and `JuliaProject.toml` markers
  as project/environment facts only;
- `scripts/` and `test/` paths by declared role, not by filename alone.

It emits the same RIPR fact families as other preview adapters and tags each
finding with `language = "julia"` and `language_status = "preview"`.

The adapter is syntax-first. It must not depend on the Julia compiler, package
manager, test runner, GPU runtime, or native library execution. When
syntax-first analysis cannot classify, the adapter emits an explicit
`static_limit_kind` (or a Julia-specific limit token defined below) and
conservative exposure classes such as `infection_unknown` or `static_unknown`.
It must not silently map unresolved dispatch, macro, or native edges to
`no_static_path`.

The language-neutral boundary, router posture, additive output metadata, and
preview policy live in
[RIPR-SPEC-0026](RIPR-SPEC-0026-language-adapter-contract.md) and
[RIPR-SPEC-0030](RIPR-SPEC-0030-preview-evidence-policy-boundary.md).
Activation vocabulary maps through the shared test-activation contract (#5542);
oracle strength maps through the shared completeness contract (#5546). Witness
and output projection reuse shared schemas (#5532); this spec does not introduce
Julia-only evidence classes, gap schemas, or support semantics.

### Soundness invariant

Unknown or limited reach caused by Julia dynamism, package ambiguity, or native
indirection is never a clean unreached gap. Multiple-dispatch ambiguity, macro
expansion, generated functions, world age, method redefinition, `eval`, package
extensions, custom test macros, and GPU/native boundaries produce typed limits
and unknown/limited classifications, not `no_static_path` merely because a
callee or activation path was unresolved.

**Reach law:** generic-function name, module import path, `Project.toml`
dependency name, or native symbol spelling alone MUST NOT establish method reach
or oracle eligibility.

### Wire language, version boundaries, and routing

| Concern | Contract |
| --- | --- |
| Wire language | `julia` |
| Feature gate | `lang-julia` |
| File routing | `.jl` production, test, script, and generated sources after role classification |
| Version evidence | Optional `compat` / `julia = "..."` bounds from `Project.toml` when parsed statically; never inherit host `julia --version` silently |
| Environment | Bounded `Manifest.toml` / `[compat]` snapshot ids with explicit `currentness`; unresolved selected environment names a limitation |

Build-only paths (`Project.toml`, `Manifest.toml`, `Artifacts.toml`, CI YAML) feed
**project facts** only and MUST NOT enter semantic adapters as mutation owners
unless a later slice explicitly classifies a script role.

### Project, package, and source roles

Portable project facts (no package resolution execution):

- **Package identity**: `name`, `uuid`, `[deps]` / `[weakdeps]` / `[extensions]` names
  as declared strings only;
- **Environment**: `Project.toml` + optional bounded manifest digest;
- **Source roles**: `production` (`src/`), `test` (`test/`, `tests/`), `script`,
  `generated`, `vendor`, `native`, `gpu`, `benchmark`, `docs`, `config`;
- **Vendor deny**: vendored trees and artifact stubs are ineligible for mutation
  seams unless explicitly opted in by a future policy slice;
- **Generated**: `@generated`, compiler output, and GPU kernel stubs require
  `source_provenance = generated` on probes.

### Owner facts and stable method identity

Owners the adapter must recognise:

- modules and submodules (`module`, `baremodule`);
- generic functions and their **method signatures** as distinct owners;
- `struct` and `mutable struct` types, inner constructors, and outer constructor
  methods;
- callable objects (`struct` + `(obj)(args...)` when syntactically bound);
- macros (`macro`, `@macro` definitions) as definition sites, not expanded bodies;
- `@generated` function definitions;
- package **extension** activation sites tied to `[extensions]` entries when
  syntactically declared.

Portable owner identities are repository-relative and language-qualified:

```text
julia:<normalized/path.jl>::<owner_path>
```

Method identity MUST include dispatch context, not merely the generic name:

```text
julia:src/foo.jl::MyModule.bar(::Int, ::Float64)
```

Examples:

- `julia:src/calc.jl::MyMod.f(::Vector{Float64})` — one method;
- `julia:src/calc.jl::MyMod.f(::Matrix{Float64})` — sibling method, distinct owner;
- `julia:src/types.jl::Point(::Float64, ::Float64)` — constructor method.

Collapsing all methods of `f` into one owner without signature keys is forbidden.
When overload sets cannot be resolved statically, emit `julia_dispatch_set_unresolved`.

Owner kinds (additive to RIPR-SPEC-0026 where needed):

- `module`, `generic_function`, `method`, `struct_type`, `mutable_struct_type`,
  `constructor`, `callable_object`, `macro_definition`, `generated_function`,
  `extension_module`.

### Probe facts

Probes attach to exact source spans. Required families include:

- predicate and guard probes (`if`, `?:`, short-circuit, `@assert` in production);
- return-value and last-expression probes;
- exception-path probes (`throw`, `@assert` failure, `try`/`catch` behavior change);
- numerical/comparison probes on changed expressions;
- array/collection mutation probes (indexed assignment, `push!`, in-place ops);
- output/logging probes (`@info`, `@warn`, `@error`, `print`, `println` when literal);
- call/interaction probes for statically resolved calls;
- property-test registration probes (syntax-only).

When classification fails, emit `static_limit_kind` instead of inventing probes.

### Julia preview fact vocabulary

| Field | Values (bounded) |
| --- | --- |
| `language` | `julia` |
| `language_status` | `preview` |
| `framework` | `test_stdlib`, `unknown_test` |
| `owner_kind` | see Owner facts |
| `test_kind` | `testset`, `test_item`, `test_throws`, `test_broken`, `test_skip`, `test_logging`, `test_warn`, `test_inference`, `generated_test`, `parameterized_test`, `unknown_test` |
| `assertion_kind` | `exact_value`, `approx_value`, `approx_array`, `array_shape`, `array_axes`, `array_order`, `element_type`, `mutation_observed`, `allocation_declared`, `inference_declared`, `exception_path`, `output_log`, `state_change`, `property_smoke`, `smoke_weak` |
| `probe_kind` | `predicate`, `return_value`, `error_path`, `numerical_compare`, `array_shape`, `array_mutation`, `output_log`, `call_side_effect`, `property_registration` |
| `static_limit_kind` | shared kinds from RIPR-SPEC-0026 plus Julia-specific tokens below |
| `activation_state` | maps to shared #5542 vocabulary |

Julia-specific `static_limit_kind` tokens:

- `julia_dispatch_ambiguous` — multiple applicable methods not fixed syntactically;
- `julia_dispatch_set_unresolved` — overload set or vararg/splat shape unknown;
- `julia_world_age` — method age / world counter effects not evidenced;
- `julia_method_redefinition` — same signature redefined across files or eval;
- `julia_macro_expansion` — macro body or hygiene not available statically;
- `julia_generated_function` — `@generated` body depends on type parameters opaquely;
- `julia_eval_metaprogramming` — `eval`, `@eval`, generated expression splicing;
- `julia_package_extension` — extension module load edge unresolved;
- `julia_custom_test_macro` — non-`Test` macro wrapping `@test`;
- `julia_opaque_property` — property macro or custom `@test` helper not on closed list;
- `julia_runtime_inference` — `@inferred`, allocation, or type-stability claims needing runtime;
- `julia_native_ccall` — `ccall`, `@ccall`, `cfunction` without #5559 edge;
- `julia_gpu_kernel` — CUDA/KernelAbstractions/GPU kernel launch without #5559 edge;
- `julia_external_state` — global refs, ENV, or process state not locally evidenced;
- `julia_manifest_unresolved` — selected environment or compat bound unknown.

Reuse shared limit kinds when accurate (`dynamic_dispatch`, `metaprogramming`,
`missing_import_graph`, `unsupported_syntax`).

### Standard `Test` registration and activation (#5542)

Map `Test` stdlib surfaces into shared activation states, not a parallel schema:

| Surface | Registration | Activation notes |
| --- | --- | --- |
| `@testset` | group | nested sets preserve hierarchy |
| `@test` | test item | |
| `@test_throws` | exception test | exception type/value must be syntactically present for strong credit |
| `@test_broken` | | `pending` / broken |
| `@test_skip` | | `skipped` |
| `@test_warn` / logging tests | | require literal or resolvable message pattern |
| `@test_logs` / `@test_nowarn` | | map to output/log oracle families |
| `@inferred` / `@allocated` / `@allocations` | | `julia_runtime_inference` unless closed static subset |
| setup blocks / `include` in `test/` | | registration only; no reach from presence alone |
| generated / parameterized tests | | `generated` / `parameterized` until case key is static |

Custom test macros MUST map to `julia_custom_test_macro` unless on a closed
inspect list defined by a future implementation slice.

### Numerical and approximate assertions

Exact and approximate observations remain **distinct facts**:

| Observation | Required fields |
| --- | --- |
| Exact equality | `assertion_kind = exact_value`, expected and actual expressions or literals |
| Approximate | `assertion_kind = approx_value`, `tolerance` (rtol/atol/norm), `observed_quantity` (which expression is compared), comparison operator |
| Type-only | weak; cannot alone credit numeric seams |

`@test a ≈ b` MUST record tolerance tokens separately from `@test a == b`.
Changing tolerance without changing the compared expression is a distinct oracle
dimension (#5546).

### Array, collection, and mutation evidence

Array facts MUST NOT collapse into one blob:

| Fact | Meaning |
| --- | --- |
| `array_shape` | size/dimension tuple |
| `array_axes` | axis labels or non-standard indexing when syntactically present |
| `array_order` | iteration order / column-major layout claims |
| `element_type` | eltype / typed array constructor |
| `exact_value` / `approx_array` | element-wise or whole-array comparisons |
| `mutation_observed` | indexed assignment, `setindex!`, in-place mutating ops on a binding tied to the changed production seam |
| `allocation_declared` | `@allocated`, `@allocations` declarations |
| `inference_declared` | `@inferred` declarations |

A shape-only `@test size(x) == (2, 2)` MUST NOT credit element-wise behavior.
An element `@test x[1] == 1` MUST NOT imply shape coverage without a shape fact.

### Relations, exceptions, logs, output, and property evidence

- Direct call edges require resolved method identity on both sides;
- Helper chains within the same file with literal arguments may reach at bounded strength;
- Exception oracles require resolvable exception types and optional message patterns;
- Log/output oracles require literal or syntactically fixed substrings;
- Property-based tests register as `property_smoke` at best until a closed oracle
  inspects the property body;
- Package extension calls require extension owner linkage or `julia_package_extension`.

### Native, GPU, and cross-language edges (#5559)

`ccall`, `@ccall`, `cfunction`, GPU kernel launches, and foreign ABI boundaries
MUST link only through a registered cross-language edge in
[RIPR-SPEC-0378](RIPR-SPEC-0378-cross-language-edge-registry-v1.md) with stable
edge id, digest, and both-side descriptors. Callee name or library string alone
is insufficient. Missing edge → `julia_native_ccall` or `julia_gpu_kernel` and shared
`cross_language_edge_missing` when using registry vocabulary.

Package extensions (`[extensions]` / weakdeps) record declared names statically;
runtime activation without evidence → `julia_package_extension`.

### Capability cell registry (normative preview index)

Capability cells use identity shape
`{language}/{framework}/{mode}/{family}` per
[RIPR-SPEC-0365](RIPR-SPEC-0365-capability-manifest-cell-join.md):

| Cell id | Support state (initial) | Tier ceiling |
| --- | --- | --- |
| `julia/test_stdlib/diff/static_facts` | `not_proven` | `preview` |
| `julia/test_stdlib/repo/static_facts` | `not_proven` | `preview` |
| `julia/any/diff/repair_routing` | `not_proven` | `preview` |
| `julia/any/repo/completeness` | `not_proven` | `preview` |

Product manifest claims for these ids are added only when
`metrics/capabilities.toml` entries explicitly cite implementation proof; this
spec registers the ids for index completeness only.

Scientific or numerical package corpora (#5560) are judged separately from
general Julia fixtures when promotion work lands; this spec requires separate
capability reporting by analysis mode but does not define corpus files here.

## Required Evidence

This spec slice is documentation-only. Satisfaction requires:

- this document with positive and negative acceptance examples for every contract
  family above;
- `docs/spec-index.d/RIPR-SPEC-0405.md` index row;
- `.ripr/traceability.d/` behavior entry;
- `metrics/capabilities.toml` and `docs/CAPABILITY_MATRIX.md` rows for
  `julia_preview_static_facts` in `planned` status;
- `cargo xtask check-spec-format`, `check-spec-numbering`, `check-doc-index`,
  `check-doc-artifacts`, `check-traceability`, `check-capabilities`, and
  `check-static-language` clean on the PR that lands the spec.

Future implementation PRs must add fixture-backed proof per #5560 before
promoting any Julia cell beyond `planned`. Acceptance examples in this document
are keyed for future Julia judged corpora as `RIPR-SPEC-0405#<slug>`.

## Non-Goals

- No Julia compiler/runtime, package manager, test suite, GPU workload, or native
  library execution during passive analysis.
- No claim that static evidence proves numerical or scientific correctness.
- No universal macro expansion, dispatch resolution, or generated-function
  evaluation.
- No parser dependency selection, adapter implementation, generated tests, or edits.
- No support-tier promotion, release, publication, gate, badge, baseline, or
  RIPR Zero authority.
- No Julia-only JSON schema fork or parallel activation/oracle enums in output.

## Acceptance Examples

Each example states expected static evidence vocabulary; none imply runtime
mutation outcomes.

### 1. Method signature identity (positive)

```julia
function norm(x::Vector{Float64})
    sum(abs2, x)
end

function norm(x::Matrix{Float64})
    sum(abs2, x)
end
```

Expected: two distinct method owners with signature keys; changing the `Vector`
method MUST NOT credit the `Matrix` method without signature match.

### 2. Generic name alone (negative)

A test calls `norm(x)` where static analysis lists two `norm` methods. Expected:
`julia_dispatch_ambiguous` or `julia_dispatch_set_unresolved`; no reach edge to
a single method from name alone.

### 3. Exact numerical oracle (positive)

```julia
@test compute(1.0, 2.0) == 3.0
```

Expected: `assertion_kind = exact_value`, strong oracle on observed call result.

### 4. Approximate oracle with tolerance (positive)

```julia
@test compute(pi) ≈ 3.14 atol=0.01
```

Expected: `approx_value` with `tolerance = atol=0.01` and `observed_quantity`
naming the compared expression; distinct from exact equality family.

### 5. Approximate without tolerance pin (negative)

```julia
@test x ≈ y
```

with no rtol/atol when defaults would matter for the changed seam. Expected: weak
or unknown numerical credit; MUST NOT read as full boundary coverage (#5546).

### 6. Array shape vs values (positive)

```julia
@test size(A) == (2, 3)
@test A[1, 1] == 1.0
```

Expected: separate `array_shape` and `exact_value` facts; neither implies the other.

### 7. Mutation vs non-mutating view (negative)

```julia
@test A == B
```

after a production change that mutates `A` in place. Expected: no
`mutation_observed` unless an assertion observes the mutated binding or indexed
update; equality alone does not prove mutation coverage.

### 8. `@test_throws` (positive)

```julia
@test_throws ArgumentError compute(-1)
```

Expected: `exception_path` oracle with resolvable `ArgumentError` type.

### 9. `@test_skip` activation (activation)

```julia
@test_skip "later" 1 == 2
```

Expected: `skipped` activation; must not suppress unrelated active tests (#5542).

### 10. Macro expansion limit (negative)

```julia
@my_macro generate_tests()
```

Expected: `julia_macro_expansion` or `julia_custom_test_macro`; no oracle credit
from macro name alone.

### 11. `@generated` limit (negative)

```julia
@generated function foo(x)
    :(x + 1)
end
```

Expected: `julia_generated_function`; no promotion without evidenced expansion body.

### 12. World age / eval limit (negative)

```julia
eval(:(f(x)))
```

Expected: `julia_eval_metaprogramming` or `julia_world_age`; not `no_static_path`.

### 13. Native `ccall` without edge (negative)

```julia
ccall(:libfoo, Cint, (Cint,), 1)
```

Expected: `julia_native_ccall` or shared `cross_language_edge_missing`; no reach
from symbol spelling.

### 14. Native `ccall` with #5559 edge (positive)

When a registered edge links Julia call site to foreign descriptor. Expected:
cross-language relation cites edge id and digest; both-side descriptors present.

### 15. GPU kernel without edge (negative)

```julia
CUDA.@cuda mykernel!(A)
```

Expected: `julia_gpu_kernel`; no callee-name reach.

### 16. Package extension declaration (positive)

`Project.toml` lists `[extensions]` entry `MyExt = "WeakDep"`. Expected: static
extension registration fact; activation unknown until evidenced or limited.

### 17. Project dependency not reach (negative)

`[deps]` lists `LinearAlgebra` but test only mentions package name in a string.
Expected: no related-test edge from dependency string.

### 18. `@allocated` declaration (distinct fact)

```julia
@test @allocated foo() == 0
```

Expected: `allocation_declared` separate from return-value and mutation facts;
`julia_runtime_inference` when static proof requires runtime counters.

### 19. Callable object owner (positive)

```julia
struct Adder
    offset::Float64
end
(a::Adder)(x) = x + a.offset
```

Expected: `callable_object` owner for `(a::Adder)(x)` method distinct from struct type.

### 20. Exact span requirement (positive)

Any emitted probe or owner fact must cite exact start/end byte or line/column
spans in adapter output metadata from the first implementation.

## Test Mapping

See `.ripr/traceability.d/` behavior `RIPR-SPEC-0405`. Executable tests land
with analysis slices named in the #5593 child graph and judged corpora. Until
then, `fixtures/*/verdict-corpus/spec-scopes/RIPR-SPEC-0405.toml` marks the spec
out of runtime mutant corpora.

## Implementation Mapping

Planned work follows the #5593 child graph (spec-only slice lands item 1):

1. This spec and scientific non-claims;
2. Parser/lowering/compiler-fact substrate ADR;
3. Routing, config, project/package, and source-role discovery;
4. Modules, generic functions, method signatures, probes, stable IDs;
5. Standard `Test` activation and oracle facts;
6. Approximate numerical, array/shape, mutation, and property discriminator facts;
7. Dispatch, macros, generated functions, eval, native/GPU edges, relations, limits;
8. Bounded repo mode and environment completeness;
9. Output/editor/agent parity;
10. Synthetic corpus, representative packages, calibration, and support-tier decision.

No module paths are normative in this slice.

## Metrics

- `language_adapter_julia_spec_registered` — spec, traceability, capability, and
  doc index rows present;
- future: per-limit counters and framework activation/oracle counters once fixtures exist.

## Failure Modes

- Collapsing methods under generic function names without signatures.
- Treating package dependencies, imports, or native symbol spellings as reach.
- Merging array shape, element values, order, eltype, and mutation into one oracle.
- Promoting approximate assertions without tolerance and observed quantity.
- Emitting `no_static_path` for unresolved dispatch, macros, or native/GPU edges.
- Registering native/GPU credit without #5559 provenance.
