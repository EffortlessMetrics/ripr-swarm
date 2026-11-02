# RIPR-SPEC-0393: .NET Shared Project Facts and Separate C#, F#, and Visual Basic Evidence Contracts

Status: accepted

Owner: product / language-adapters

Created: 2026-11-02

Linked issues:

- [#5632](https://github.com/EffortlessMetrics/ripr-swarm/issues/5632) — normative .NET family contract (this document)
- [#5585](https://github.com/EffortlessMetrics/ripr-swarm/issues/5585) — .NET language epic (parent)
- [#5582](https://github.com/EffortlessMetrics/ripr-swarm/issues/5582) — 22-ecosystem language-expansion programme

Shared architecture consumed by reference (not implemented here):

- [#5526](https://github.com/EffortlessMetrics/ripr-swarm/issues/5526) — shared multi-language architecture
- [#5531](https://github.com/EffortlessMetrics/ripr-swarm/issues/5531), [#5532](https://github.com/EffortlessMetrics/ripr-swarm/issues/5532),
  [#5541](https://github.com/EffortlessMetrics/ripr-swarm/issues/5541), [#5542](https://github.com/EffortlessMetrics/ripr-swarm/issues/5542),
  [#5546](https://github.com/EffortlessMetrics/ripr-swarm/issues/5546), [#5559](https://github.com/EffortlessMetrics/ripr-swarm/issues/5559),
  [#5560](https://github.com/EffortlessMetrics/ripr-swarm/issues/5560)

Linked specs:

- [RIPR-SPEC-0026](RIPR-SPEC-0026-language-adapter-contract.md) — language-neutral adapter boundary
- [RIPR-SPEC-0378](RIPR-SPEC-0378-cross-language-edge-registry-v1.md) — registered cross-language edges (#5559)

Support-tier impact:

- **No tier change.** C#, F#, and Visual Basic remain **preview/advisory** per
  language, framework, and analysis mode until separate eval and support PRs
  cite proof. This spec defines contracts only; it does not default-enable any
  adapter, gate, badge, baseline, or release authority. The canonical boundary
  remains [support tiers](../status/SUPPORT_TIERS.md).

Policy impact:

- Register in `policy/doc-artifacts.d/RIPR-SPEC-0393.toml`, `docs/spec-index.d/RIPR-SPEC-0393.md`,
  and `.ripr/traceability.d/RIPR-SPEC-0393.toml`.
- No new runtime crates, binaries, MSBuild/Roslyn/F# compiler execution, NuGet
  restore, `dotnet test`, or support-tier promotion.

## Problem

The .NET family shares MSBuild project structure, target frameworks, package
references, and several test frameworks, but **C#, F#, and Visual Basic are not
one semantic language**. Collapsing them into a synthetic `dotnet` analyzer would
assign one language's owners, activation rules, oracles, and limitations to
another and would violate the multi-language architecture in #5526.

Before compiler-service or adapter code lands (#5633–#5645), RIPR needs one
normative contract that:

1. Defines reusable **solution/project/build facts** independent of any single
   language adapter.
2. Defines **three separate semantic evidence laws** for C#, F#, and Visual Basic.
3. Maps test-framework activation and oracle eligibility without erasing language
   identity.
4. Names typed stops for dispatch, reflection, generation, inference, late
   binding, and unresolved target/configuration context.

## Behavior

### Wire identities and routing

RIPR uses **discrete language codes** only. There is no synthetic `dotnet`
language code for findings, capability cells, or owner identifiers.

| Wire code | Source extensions | Project files |
| --- | --- | --- |
| `csharp` | `.cs` | `.csproj` |
| `fsharp` | `.fs`, `.fsi`, `.fsx` | `.fsproj` |
| `visualbasic` | `.vb` | `.vbproj` |

Shared infrastructure may consume `.sln`, `.slnx`, and MSBuild XML for **project
facts** only. Semantic adapters retain authority for owners, probes, oracles, and
limitations within their wire code.

A file routes to at most one semantic adapter by extension. Cross-language calls
and references are **edges**, not owner inheritance (see Cross-language edges).

### Non-execution invariant

Passive analysis **must not** run MSBuild targets, `dotnet restore`, `dotnet build`,
`dotnet test`, NuGet restore, Roslyn design-time builds, F# compiler services, or
Visual Basic compile pipelines as authority. Facts come from **declared** solution,
project, and source text plus approved exported fact inputs defined in a later ADR
(#5633). Ambient machine state (GAC, user NuGet cache, IDE bin/obj) is not evidence.

### Shared .NET project fact contract

Schema version:

```text
ripr-dotnet-project-facts.v1
```

The project substrate is **machine-readable** and **language-neutral**. Language
adapters consume it; they do not redefine project identity.

#### Portable identities and currentness

Each fact carries:

- `fact_id` — stable within the export
- `currentness` — `exact` | `partial` | `stale` | `unknown` with optional
  `currentness_basis` (for example `declared_only`, `exported_compiler_snapshot`)
- `project_context_id` — binds facts to one evaluated project/configuration slice

Project contexts are keyed by:

```text
<repo_relative_project_path>#<configuration>#<target_framework_moniker>
```

When a TFM or configuration cannot be resolved to a **single** slice, facts for
that slice use `currentness = unknown` and **must not** back a universal claim
across TFMs.

#### Solution and project discovery

Facts cover, without execution:

| Family | Declared inputs |
| --- | --- |
| Solution | `.sln`, `.slnx` membership, nested solution folders |
| Project | `.csproj`, `.fsproj`, `.vbproj` identity, SDK style vs legacy |
| References | `ProjectReference`, `PackageReference`, `Reference` (assembly) |
| Targeting | `TargetFramework`, `TargetFrameworks`, `TargetFrameworkIdentifier`, implicit TFMs |
| Configuration | `Configuration`, conditional `PropertyGroup` / `ItemGroup` |
| Compile items | `Compile`, `EmbeddedResource`, `Content`, `None`, linked paths (`Link` metadata) |
| Generated roles | `Generator`, `CustomTool`, source-generator / analyzer package signals (declared only) |
| Test signals | Test SDK packages, `IsTestProject`, `Microsoft.NET.Test.Sdk`, adapter packages |
| Roots | Repository-selected analysis roots (#5526); excluded paths remain explicit |

#### Multi-targeting and conditionals

- Multi-targeted projects (`TargetFrameworks`) yield **one context row per TFM**
  that the export can name from declared properties and known constants.
- Conditional items (`Condition="'$(Configuration)'=='Release'"`) attach to the
  contexts the export can prove; otherwise `currentness = partial` or `unknown`.
- Linked files, shared projects (`.shproj` / `.projitems`), and wildcard globs
  resolve to explicit paths when the export lists expanded items; otherwise the
  export records `compile_item_unexpanded` limitation rather than guessing.

#### Package and project graph

Package and project graph facts record **declared** IDs and versions from project
XML and lockfiles **when present in the repository**. Missing lockfiles or
unresolved version ranges become `package_version_unknown` — not inferred from the
network or cache.

**Reach law:** a `ProjectReference`, `PackageReference`, shared assembly reference,
or shared target membership **does not** establish changed-owner reach, test
oracle credit, or cross-language owner assignment. It may inform discovery and
registered edges only.

#### Machine-readable project fact envelope

Normative shape (fields may grow in `v1.x` with explicit schema bumps):

```toml
schema_version = "ripr-dotnet-project-facts.v1"
proof_generation = "proof.v1"
currentness_generation = "currentness.v1"

[[projects]]
project_id = "repo:relative/path/App.csproj"
language = "csharp" # project primary language, not finding language
sdk_style = true
target_frameworks = ["net8.0", "net48"]
configurations = ["Debug", "Release"]

[[projects.compile_items]]
path = "src/Pricing.cs"
role = "handwritten" # handwritten | generated | linked | unknown
generator = "" # nonempty when declared
link_target = "" # nonempty when Link metadata present
contexts = ["App.csproj#Debug#net8.0"]

[[projects.references]]
kind = "project" # project | package | assembly
target_project_id = "repo:relative/path/Contracts.csproj"
version = ""
contexts = ["App.csproj#Debug#net8.0"]

[[projects.test_signals]]
framework_hint = "xunit" # declared package/id only
contexts = ["Tests.csproj#Debug#net8.0"]
```

### Semantic evidence contracts (per language)

Each language contract is **separately machine-readable** under its own section
ID. No language inherits another's owner grammar, activation rules, oracle
families, or support claims.

| Contract ID | Wire code | Document section |
| --- | --- | --- |
| `ripr-csharp-evidence.v1` | `csharp` | C# evidence law |
| `ripr-fsharp-evidence.v1` | `fsharp` | F# evidence law |
| `ripr-visualbasic-evidence.v1` | `visualbasic` | Visual Basic evidence law |

Shared rules for all three:

1. **Exact spans** — every probe and oracle binding cites UTF-16 source spans
   `(start, end)` in the file snapshot analyzed; byte offsets are not interchangeable.
2. **Signature-aware identity** — owners and callees use stable IDs including
   parameter arity, type slots (when resolved), generic arity, and static/instance;
   overload sets must not collapse without an explicit resolved member.
3. **Conditional compilation** — `#if`, `#elif`, `#else`, `#endif`, and file-level
   `#define`/`/define:CONSTANT` attach probes to **named** symbol contexts; unresolved
   constant truth → `configuration_unknown` limitation, not a merged owner.
4. **Activation before oracle** — test activation state is decided before any oracle
   strength or exposure class that depends on an executed test body.

#### C# evidence law (`ripr-csharp-evidence.v1`)

**Owner kinds** (non-exhaustive v1 vocabulary):

`namespace`, `type`, `record`, `enum`, `delegate`, `method`, `constructor`,
`operator`, `property`, `event`, `indexer`, `local_function`, `lambda`, `field`.

**Identity must disambiguate:**

- partial types and partial methods (file-local part identity)
- generic types and methods (type parameters and arity)
- extension methods (reduced vs unreduced signature)
- explicit interface implementations
- async state-machine entry points vs async method owner
- iterator / yield return owners
- pattern-based properties and `init` accessors
- attributes affecting analysis (for example `Obsolete`, `Conditional`)
- `dynamic` sites and expression-tree construction (typed limits, not reach guesses)
- source-generated members (only when export marks `generated` with generator id)

**Probe families** (exact-span behavior sites):

`if_else`, `switch_statement`, `switch_expression`, `throw`, `return`,
`assignment`, `pattern_guard`, `null_coalescing`, `await`, `yield_return`.

**Typed limits (non-exhaustive):**

`overload_unresolved`, `generic_constraint_unresolved`, `virtual_dispatch_unresolved`,
`interface_dispatch_unresolved`, `dynamic_dispatch`, `reflection_route`,
`source_generator_opaque`, `di_proxy_opaque`, `configuration_unknown`,
`target_framework_unknown`.

#### F# evidence law (`ripr-fsharp-evidence.v1`)

**Owner kinds:**

`module`, `namespace`, `type`, `union_case`, `record_label`, `member`, `function`,
`value`, `active_pattern`, `computation_expression_builder`.

**Identity must disambiguate:**

- module vs namespace nesting and `RequireQualifiedAccess`
- curried signatures vs tupled forms at call sites
- type inference gaps (generalization limits)
- inline and statically resolved member constraints
- async/task CE builders vs raw `Async`/`Task` owners
- quotations and splice sites
- type providers (declared provider id or `type_provider_opaque`)
- units-of-measure annotations on types and values
- interop `CLIMutable`, `Struct`, `Sealed` patterns at boundaries

**Probe families:**

`match`, `if_then_else`, `failwith`, `raise`, `return`, `yield`, `pipeline_step`.

**Typed limits:**

`inference_unresolved`, `computation_expression_unresolved`, `type_provider_opaque`,
`quotation_opaque`, `interop_dispatch_unresolved`, `configuration_unknown`,
`target_framework_unknown`.

#### Visual Basic evidence law (`ripr-visualbasic-evidence.v1`)

**Owner kinds:**

`namespace`, `module`, `class`, `structure`, `interface`, `enum`, `delegate`,
`method`, `sub`, `function`, `property`, `event`, `operator`, `lambda`.

**Identity must disambiguate:**

- default properties and `Default` metadata
- `Handles` / `WithEvents` event wiring
- optional / named arguments at call sites
- late binding when `Option Strict Off` (`late_binding` limit)
- XML literal expressions
- legacy My.* and VB-specific project types

**Probe families:**

`if_then_else`, `select_case`, `throw`, `return`, `assignment`, `raise_event`.

**Typed limits:**

`late_binding`, `default_property_unresolved`, `overload_unresolved`,
`reflection_route`, `configuration_unknown`, `target_framework_unknown`.

### Test and oracle contract

Test facts are **language-scoped** even when frameworks share attribute names.

#### C# and Visual Basic (shared framework shapes, separate language identity)

| Framework | Registration signals (declarative) | Activation states |
| --- | --- | --- |
| xUnit | `[Fact]`, `[Theory]`, `[InlineData]`, `[MemberData]` | `active`, `skipped`, `ignored`, `explicit`, `category_filtered`, `platform_filtered`, `unknown` |
| NUnit | `[Test]`, `[TestCase]`, `[TestCaseSource]`, `[Ignore]`, `[Explicit]` | same vocabulary |
| MSTest | `[TestMethod]`, `[DataTestMethod]`, `[DataRow]`, `[Ignore]`, `[DeploymentItem]` | same vocabulary |

Rules:

- **Registration ≠ active case** — theories, property rows, and generated cases
  remain distinct from known active invocations until activation is established.
- **Theory / property / generated** — `[Theory]` / `[DataTestMethod]` / FsCheck
  properties carry `case_kind = theory_row | generated_case | registered_only`.
- **Fixtures and lifetimes** — class/method assembly fixtures (`IClassFixture`,
  NUnit `SetUpFixture`, MSTest `ClassInitialize`) affect activation only when
  statically visible; otherwise `fixture_lifetime_unknown`.
- **Expected exceptions** — map to oracle family `exact_exception` only with
  exact exception type and optional message pattern per language rules.

JUnit-style naming similarity **does not** merge C# and VB test identity.

#### F# (separate frameworks)

| Framework | Registration signals | Notes |
| --- | --- | --- |
| Expecto | `testCase`, `testProperty`, `testList`, `testMany` | FsCheck properties via Expecto wrappers stay F#-scoped |
| FsCheck | `Property.check`, `Prop.forAll`, `[<Property>]` | Distinct from xUnit theories |

Expecto and FsCheck oracles **must not** credit C# or VB owners without a
registered cross-language edge (#5559, RIPR-SPEC-0378).

#### Oracle eligibility order

```text
test_discovered
-> activation_established | activation_unknown | activation_inactive
-> oracle_admitted | oracle_refused
-> exposure classification (existing RIPR classes; preview capped)
```

When activation is not `activation_established`, oracle families are withheld and
the limitation `test_activation_unestablished` applies (same law as RIPR-SPEC-0028).

### Cross-language edges

Cross-project and cross-language behavior uses
[RIPR-SPEC-0378](RIPR-SPEC-0378-cross-language-edge-registry-v1.md) and #5559.

- A C# call into an F# module, or VB `Imports` of a C# namespace, requires a
  **registered edge** with producer basis and strength ceiling.
- Project reference alone records `project_reference` fact only.
- P/Invoke, COM, reflection-only loads, DI-resolved interfaces, and runtime proxies
  default to `reflection_route` or `di_proxy_opaque` unless exported facts name a
  resolved target with evidence refs.

### Limitation and authority law

Initial support posture per capability cell (see registry below):

- `support_state = not_proven`
- `language_status = preview`
- `gate_impact = none`
- `repair_packet_ready = false` for all preview findings until a later spec promotes
  a bounded route with proof

Typed stops listed in this spec are **closed vocabularies** at first implementation;
unknown producer kinds fail closed as `static_unknown`.

Stryker.NET and other mutation runners may calibrate later eval (#5645) but are
**never invoked** during passive analysis.

### Capability cell registry (planned)

These cells are registered in this spec and traceability for #5632 acceptance.
They **must not** appear in `CAPABILITY_CELLS` or default `[languages]` until
implementation and eval PRs land.

```toml
# registry: ripr-dotnet-capability-cells.v1 (normative planning table)

[[cell]]
id = "csharp/xunit/diff/static_facts"
language = "csharp"
framework = "xunit"
mode = "diff"
family = "static_facts"
support_state = "not_proven"

[[cell]]
id = "csharp/nunit/diff/static_facts"
language = "csharp"
framework = "nunit"
mode = "diff"
family = "static_facts"
support_state = "not_proven"

[[cell]]
id = "csharp/mstest/diff/static_facts"
language = "csharp"
framework = "mstest"
mode = "diff"
family = "static_facts"
support_state = "not_proven"

[[cell]]
id = "fsharp/expecto/diff/static_facts"
language = "fsharp"
framework = "expecto"
mode = "diff"
family = "static_facts"
support_state = "not_proven"

[[cell]]
id = "fsharp/fscheck/diff/static_facts"
language = "fsharp"
framework = "fscheck"
mode = "diff"
family = "static_facts"
support_state = "not_proven"

[[cell]]
id = "visualbasic/xunit/diff/static_facts"
language = "visualbasic"
framework = "xunit"
mode = "diff"
family = "static_facts"
support_state = "not_proven"

[[cell]]
id = "visualbasic/nunit/diff/static_facts"
language = "visualbasic"
framework = "nunit"
mode = "diff"
family = "static_facts"
support_state = "not_proven"

[[cell]]
id = "visualbasic/mstest/diff/static_facts"
language = "visualbasic"
framework = "mstest"
mode = "diff"
family = "static_facts"
support_state = "not_proven"

[[cell]]
id = "dotnet/any/repo/project_facts"
language = "dotnet"
framework = "any"
mode = "repo"
family = "project_facts"
support_state = "not_proven"
note = "Substrate-only cell; never emits semantic C#/F#/VB owner findings."
```

The substrate row uses `language = dotnet` **only** for project-fact exports, not for
semantic findings. Semantic cells remain `csharp`, `fsharp`, and `visualbasic`.

## Required Evidence

- `fixtures/dotnet-evidence-contract-corpus/corpus.json` pins positive and negative
  acceptance shapes for project facts, the three language contracts, activation order,
  and limitation stops.
- Future adapter PRs must attach discriminating tests per `Test Mapping`; until then
  the corpus and spec-format gates are the binding evidence for this slice.

## Non-Goals

- MSBuild, Roslyn, F# compiler, or VB compiler execution during passive analysis
- NuGet restore, `dotnet test`, or Stryker.NET invocation
- Adapter, parser, routing, or Rust implementation (issues #5633–#5645)
- Default `[languages]` enablement, gate promotion, badge/baseline authority, or
  package publication
- Collapsing C#, F#, and VB into one semantic analyzer

## Acceptance Examples

Corpus ids in `fixtures/dotnet-evidence-contract-corpus/corpus.json` mirror these
normative examples.

1. **Multi-targeting** — `App.csproj` declares `net8.0;net48`; pricing logic differs
   by `#if NET48`; export yields two contexts; a probe on the `net48`-only arm carries
   `App.csproj#Debug#net48` only; claiming `exposed` on both TFMs without proof is
   **rejected** (`target_framework_unknown`).
2. **Generated code** — a source-generated partial method in `obj/` is tagged
   `role = generated` with generator id; semantic owner id includes generator scope;
   guessing the handwritten twin owner is **rejected** (`source_generator_opaque`).
3. **Overload resolution** — two `ApplyDiscount` methods differ by parameter types; a
   call site binds one overload by static types; crediting the other overload is
   **rejected** (`overload_unresolved`).
4. **Property/event** — a changed property getter span is distinct from its backing
   field; an oracle on the property observes the changed sink; backing-field-only
   oracle is **rejected** for property change credit.
5. **Theory registration** — `[Theory][InlineData(1)]` registers a case kind
   `registered_only` until activation is established; treating inline data as executed
   oracle without activation is **rejected** (`test_activation_unestablished`).
6. **Cross-language call** — C# calls `Lib.FSharpModule.apply` with a registered edge
   `manual_registered_bridge`; without edge, classification is `no_static_path` or
   named limit, not C# `exposed` via project reference alone.
7. **F# inference** — a call site lacks resolved type for an overloaded F# function;
   export emits `inference_unresolved`; no fabricated owner id.
8. **VB late binding** — `Option Strict Off` call through `Object`; limit
   `late_binding`; no static owner reach through dispatch target.
9. **Package graph without lock** — `PackageReference` without lock/version resolution
   yields `package_version_unknown`; no invented package content facts.
10. **Shared test framework** — MSTest `[TestMethod]` in `.vb` files carries
    `language = visualbasic`; the same attribute in `.cs` carries `language = csharp`;
    merging identities is **rejected**.

## Test Mapping

Planned tests:

- `dotnet_project_facts_multi_target_contexts_are_disjoint`
- `dotnet_generated_member_requires_generator_scope`
- `csharp_overload_identity_separates_signatures`
- `csharp_property_probe_span_excludes_backing_field`
- `dotnet_theory_registration_without_activation_withholds_oracle`
- `cross_language_call_without_edge_is_not_exposed`
- `fsharp_inference_unresolved_emits_limit_not_owner_guess`
- `visualbasic_late_binding_emits_limit`
- `package_reference_without_lock_is_unknown_not_invented`
- `mstest_vb_and_cs_test_identities_remain_language_scoped`

Until #5634–#5645 land, `fixtures/dotnet-evidence-contract-corpus/corpus.json` is the
executable contract check for this spec slice.

## Implementation Mapping

Future work (out of scope for #5632):

- #5633 — ADR for MSBuild/Roslyn/F#/VB fact substrate
- #5634 — identities, gates, routing scaffold
- #5635 — project model implementation
- #5636, #5637, #5639 — language owner/probe adapters
- #5640, #5641 — test-framework maps
- #5642–#5645 — relations, repo mode, output, eval

No production module claims this spec until those issues cite it.

## Metrics

- `boundary_disclosure` — count of named .NET limitations vs silent drops (future)
- `unit_test_pass_rate` — for corpus/schema checks when wired (#5634)
