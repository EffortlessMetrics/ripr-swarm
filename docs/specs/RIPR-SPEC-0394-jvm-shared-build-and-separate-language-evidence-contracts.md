# RIPR-SPEC-0394: JVM shared build/classpath facts and separate Java, Kotlin, and Scala evidence contracts

Status: proposed

Owner: language-adapter / ecosystem (#5586)

Created: 2026-11-02

Linked issues:

- [#5646](https://github.com/EffortlessMetrics/ripr-swarm/issues/5646) (this contract)
- Parent epic [#5586](https://github.com/EffortlessMetrics/ripr-swarm/issues/5586)
- Programme [#5582](https://github.com/EffortlessMetrics/ripr-swarm/issues/5582)
- Shared architecture [#5526](https://github.com/EffortlessMetrics/ripr-swarm/issues/5526)
- Consumed authorities: [#5531](https://github.com/EffortlessMetrics/ripr-swarm/issues/5531), [#5532](https://github.com/EffortlessMetrics/ripr-swarm/issues/5532), [#5541](https://github.com/EffortlessMetrics/ripr-swarm/issues/5541), [#5542](https://github.com/EffortlessMetrics/ripr-swarm/issues/5542), [#5546](https://github.com/EffortlessMetrics/ripr-swarm/issues/5546), [#5559](https://github.com/EffortlessMetrics/ripr-swarm/issues/5559), [#5560](https://github.com/EffortlessMetrics/ripr-swarm/issues/5560)
- Separate consumers (project facts only, never Java semantics): Clojure [#5587](https://github.com/EffortlessMetrics/ripr-swarm/issues/5587), Groovy/Spock [#5588](https://github.com/EffortlessMetrics/ripr-swarm/issues/5588)

Linked specs:

- [RIPR-SPEC-0026](RIPR-SPEC-0026-language-adapter-contract.md) (language-neutral adapter boundary)
- [RIPR-SPEC-0030](RIPR-SPEC-0030-preview-evidence-policy-boundary.md) (preview posture)
- [RIPR-SPEC-0378](RIPR-SPEC-0378-cross-language-edge-registry-v1.md) (registered cross-language edges)

Support-tier impact:

- This spec does not enable JVM languages, promote support tiers, arm default
  gates, badges, baselines, RIPR Zero blocking, or publication. All Java,
  Kotlin, and Scala evidence governed here remains preview/advisory per language,
  framework, and mode until separate promotion work cites proof. The canonical
  support-tier boundary remains
  [support tiers](../status/SUPPORT_TIERS.md).
- Clojure and Spock campaigns reuse only the shared substrate facts named in
  this document; they never inherit Java routing, owners, test semantics,
  findings completeness, or support tier from this spec.

Policy impact:

- Register in `policy/doc-artifacts.d/RIPR-SPEC-0394.toml`.
- Add `docs/spec-index.d/RIPR-SPEC-0394.md`.
- Register `.ripr/traceability.toml` behavior `RIPR-SPEC-0394`.
- Register `fixtures/*/verdict-corpus/spec-scopes/RIPR-SPEC-0394.toml` as out of
  scope for runtime mutant corpora until JVM judged corpora land (#5659).
- No adapter, parser, routing, feature gate, or `LanguageId` registration is
  introduced by this document alone.

## Problem

The JVM family shares Maven, Gradle, and sbt project shapes, classpath graphs,
and bytecode-visible symbols, but Java, Kotlin, and Scala remain distinct
semantic authorities. Without a normative contract, passive analysis could
flatten overloads into names, treat classpath membership as test reach, or
promote Groovy/Clojure co-location into Java support.

Epic #5586 needs one reusable **project/build substrate** law and three
separate **language evidence** laws before ADR selection (#5647), routing (#5648),
or adapters ship.

## Behavior

### Wire languages and routing

The closed wire language set is exactly:

```text
java
kotlin
scala
```

There is no synthetic `jvm` language code, combined owner namespace, or shared
semantic support row. File routing maps production and test sources by extension **after** build-script
exclusion:

| Extension | Wire language |
| --- | --- |
| `.java` | `java` |
| `.kt` | `kotlin` |
| `.scala` | `scala` |

Build configuration paths feed **substrate facts** only and MUST NOT enter
semantic adapters, even when their extension matches Kotlin or Scala:

- Maven: `pom.xml`
- Gradle: `build.gradle`, `build.gradle.kts`, `settings.gradle`,
  `settings.gradle.kts`, convention plugin sources under `buildSrc/` and
  `gradle/` when they are build-logic roots
- sbt: `build.sbt`, `project/*.scala`, `project/*.sbt`

The `.kts` extension on `build.gradle.kts` and `.scala` under `project/` are
substrate inputs, not Kotlin/Scala production owners. Only source roots
declared by the substrate (for example `src/main/kotlin`, `src/test/java`) map
to semantic adapters.

### Three machine-readable contracts

This spec defines three separable JSON-shaped contracts. Implementations may
store them in one envelope, but validation and promotion MUST keep the slices
independent:

| Contract | `schema_version` | Owns |
| --- | --- | --- |
| Shared JVM project/build substrate | `jvm_project_substrate.v1` | Modules, source sets, classpaths, compatibility, generated roles, test-module signals |
| Language semantic evidence | `jvm_language_evidence.v1` | Owners, probes, limitations, compiler/target context for one wire language |
| Language test/oracle evidence | `jvm_test_oracle_evidence.v1` | Registration, activation, oracle families for one wire language and one framework row |

Each document requires `validate()` after deserialization, uses
`deny_unknown_fields`, and records explicit `currentness` for every fact that
can go stale (classpath snapshot, selected build variant, compiler plugin set).

Substrate facts MUST NOT appear inside language semantic documents except by
reference (`substrate_ref` id + digest). Language documents MUST NOT claim
reach, exposure, or oracle strength from substrate fields alone.

#### Shared substrate (`jvm_project_substrate.v1`)

Portable identities and currentness cover, without executing builds:

- **Build systems**: Maven (`pom.xml` modules), Gradle (`build.gradle`,
  `build.gradle.kts`, included builds), sbt (`build.sbt`, `project/*.scala`).
- **Modules**: stable `module_id` (build-system-specific coordinates + repo-relative
  root path digest).
- **Source sets / configurations / scopes**: production, test, integrationTest,
  custom names; each carries `role` (`production`, `test`, `generated`,
  `resource`, `unknown`).
- **Compatibility**: declared `sourceCompatibility`, `targetCompatibility`,
  `jvmTarget`, `release`, Scala `scalaVersion`, Kotlin `languageVersion` when
  declared statically.
- **Classpath candidates**: project dependencies, module outputs, and resource
  roots as **candidates** with `classpath_kind` (`compile`, `runtime`, `test`,
  `annotation_processor`, `unknown`). Candidates are not reach edges.
- **Generated inputs**: annotation processor outputs, KSP/KAPT, Scala macros,
  Avro/Protobuf plugin dirs — each tagged `generated_role` and linked to the
  triggering config when statically known.
- **Multi-release / JPMS**: `module-info.java` presence, `Multi-Release` manifest
  signal, and per-release variant ids when declared.
- **Mixed-language order**: documented compilation order among source sets when
  the build file states it; otherwise `compilation_order: unknown`.
- **Repository-selected root**: the exact module/root chosen for analysis with
  `selection_basis` (`explicit_config`, `single_module`, `aggregator_nearest`,
  `ambiguous`).

**Substrate non-reach law:** A dependency edge, classpath entry, package name,
artifact coordinate, bytecode symbol, or co-located source set MUST NOT alone
establish production owner identity, test-to-owner reach, or oracle eligibility.

#### Java semantic contract (`jvm_language_evidence.v1`, `language = java`)

Stable owner identities (signature-aware, exact-span probes mandatory):

- Packages and JPMS modules; classes, interfaces, records, enums, annotations.
- Fields; methods and constructors including overload disambiguation by parameter
  types, varargs, type parameters, and visibility.
- Nested, local, and anonymous classes; lambdas and method references; sealed
  hierarchies and permitted subclasses; pattern matching and switch constructs;
  default and static interface methods.
- Reflection, proxies, dependency injection, annotation processors, and generated
  code carry `source_provenance` (`authored`, `generated`, `unknown`).

Each changed behavior probe binds to an **exact source span** and an **owner
signature** (not a simple name). Generic erasure and overload sets MUST be
represented explicitly; unresolved overload sets emit typed limitations rather
than guessing a callee.

#### Kotlin semantic contract (`jvm_language_evidence.v1`, `language = kotlin`)

Owners and limits include:

- Packages; classes, interfaces, objects, companions; data/value/sealed classes.
- Top-level and local functions; constructors; properties, accessors, delegates.
- Extension functions and properties; operators; `suspend` functions and coroutine
  scopes; inline/reified call sites; lambdas and function references.
- Nullability and smart casts; destructuring; DSL lambdas with receiver;
  multiplatform `expect`/`actual`; compiler plugins; reflection; generated code.

Coroutine, DSL, plugin, and multiplatform indirection MUST map to typed
limitations when static resolution is incomplete.

#### Scala semantic contract (`jvm_language_evidence.v1`, `language = scala`)

Owners and limits include:

- Packages; classes, traits, objects, enums; defs, vals, vars, givens,
  implicits (Scala 2), `using`/`given` (Scala 3), extensions and exports.
- Constructors; methods and operators; overloads and type members; case classes
  and companions; pattern matches; for-comprehensions; higher-order and partial
  functions; futures/effects; typeclasses; macros, inline, and quotes; compiler
  plugins; generated code.

Implicit/given resolution, macro expansion, and effect system indirection MUST
fail closed with named limitations when not evidenced.

### Test and oracle contract (`jvm_test_oracle_evidence.v1`)

Test facts are modeled **per wire language and per framework row**. Framework
syntax MUST NOT erase production language or compiler context.

**Activation precedes oracle eligibility** under [#5542](https://github.com/EffortlessMetrics/ripr-swarm/issues/5542):
registration, nesting, parameterization, dynamism, selection, tags/categories,
platform/version limits, fixtures, and lifecycle bindings are distinct fields.
Oracle credit is impossible when activation state is `disabled`, `ignored`,
`pending`, `unselected`, `unknown`, or `deferred`.

| Wire language | Framework rows (initial scope) |
| --- | --- |
| `java` | JUnit 4, JUnit 5, TestNG |
| `kotlin` | JUnit 5, Kotest (bounded: `StringSpec`, `FunSpec`, `WordSpec` entry styles) |
| `scala` | ScalaTest (`AnyFunSuite`, `AnyFlatSpec`, `Matchers`), MUnit (`FunSuite`) |

Oracle families (where statically established): equality, exception, collection,
state, output, property, interaction, and asynchronous/coroutine/effect witnesses.
Each oracle cites the **exact assertion/matcher span** and the **activated test
case identity** (including parameterized case key when present).

Parameterized, dynamic/generated, nested, tagged, disabled, ignored, pending,
and platform-limited tests retain explicit uncertainty fields; they MUST NOT
collapse to a single “active test” boolean.

### Cross-module and cross-language relations

**Same wire language, different module:** Test-to-production reach within one
wire language (for example a Java test module calling a Java class in another
Maven module) uses substrate module linkage plus an evidenced call path and test
activation under [#5542](https://github.com/EffortlessMetrics/ripr-swarm/issues/5542).
It does **not** use [RIPR-SPEC-0378](RIPR-SPEC-0378-cross-language-edge-registry-v1.md)
edges, because that registry rejects same-language positive edges.

**Different wire languages:** Cross-language production/test relations require a
registered edge under RIPR-SPEC-0378
([#5559](https://github.com/EffortlessMetrics/ripr-swarm/issues/5559)). Java
calling Kotlin or Scala (and reverse) uses JVM signature identity in the edge
payload; co-location, shared classpath, or same-module cohabitation alone is
insufficient.

### Clojure and Spock reuse boundaries

- **May consume**: `jvm_project_substrate.v1` module/source-set/classpath/
  generated-role facts and registered edges that name their own wire language.
- **Must not inherit**: Java/Kotlin/Scala owner routing, semantic probes,
  test/oracle law, exposure classifications, completeness, or support tier rows
  defined in this spec.
- Counting Clojure (#5587) or Groovy/Spock (#5588) work as Java support is
  forbidden.

### Limitation and authority law

Typed stops (non-exhaustive; implementations extend only via spec amendment):

| Limitation kind | When |
| --- | --- |
| `jvm_build_context_unresolved` | Ambiguous selected module, variant, or classpath snapshot |
| `jvm_overload_set_unresolved` | Java/Kotlin/Scala overload or generic dispatch not fixed |
| `jvm_extension_or_implicit_unresolved` | Kotlin extension or Scala implicit/given/typeclass path unknown |
| `jvm_reflection_or_di_unresolved` | Reflection, proxies, Spring/Guice/Dagger indirection |
| `jvm_generated_provenance_unknown` | Generated body without processor/plugin linkage |
| `jvm_coroutine_or_async_unresolved` | Suspend/coroutine/flow or Scala effect boundary unclear |
| `jvm_macro_or_inline_unresolved` | Scala macro/inline/quotes or Kotlin inline/reified expansion unknown |
| `jvm_cross_language_edge_missing` | Cross-language call without #5559 edge |
| `jvm_test_activation_unknown` | Registered test with unknown activation/selection state |

Unresolved build/compiler/classpath context MUST NOT become a universal
exposure claim across languages.

Initial posture: preview/advisory per language/framework/mode; no default
feature enablement; no gate, badge, baseline, RIPR Zero, release, or publication
authority from this spec alone.

### Capability cell registry (normative preview index)

Capability cells use identity shape
`{language}/{framework}/{mode}/{family}` per
[RIPR-SPEC-0365](RIPR-SPEC-0365-capability-manifest-cell-join.md). The table
registers every initial JVM cell for docs/traceability/manifest joins;
runtime registry entries land in later implementation slices (#5648, #5659).

| Cell id | Support state (initial) | Tier ceiling |
| --- | --- | --- |
| `java/junit4/diff/static_facts` | `not_proven` | `preview` |
| `java/junit5/diff/static_facts` | `not_proven` | `preview` |
| `java/testng/diff/static_facts` | `not_proven` | `preview` |
| `java/any/repo/static_facts` | `not_proven` | `preview` |
| `java/any/diff/repair_routing` | `not_proven` | `preview` |
| `kotlin/junit5/diff/static_facts` | `not_proven` | `preview` |
| `kotlin/kotest/diff/static_facts` | `not_proven` | `preview` |
| `kotlin/any/repo/static_facts` | `not_proven` | `preview` |
| `kotlin/any/diff/repair_routing` | `not_proven` | `preview` |
| `scala/scalatest/diff/static_facts` | `not_proven` | `preview` |
| `scala/munit/diff/static_facts` | `not_proven` | `preview` |
| `scala/any/repo/static_facts` | `not_proven` | `preview` |
| `scala/any/diff/repair_routing` | `not_proven` | `preview` |

Product manifest claims for these ids are added only when
`metrics/capabilities.toml` entries explicitly cite implementation proof; this
spec registers the ids for index completeness only.

## Required Evidence

- `cargo xtask check-spec-format`, `check-spec-numbering`, `check-doc-artifacts`,
  and `check-traceability` pass with this spec and its index registrations.
- Acceptance examples in this document are keyed for future JVM judged corpora
  (#5659) as `RIPR-SPEC-0394#<slug>`.
- Implementation slices (#5647–#5658) must add executable proof; this document
  alone does not satisfy runtime or adapter evidence.

## Non-Goals

- No Maven/Gradle/sbt/compiler/test/mutation execution during passive analysis.
- No universal bytecode-to-source semantic reconstruction.
- No adapter code, routing, feature gates, default enablement, or support-tier
  promotion.
- No requirement that Java, Kotlin, and Scala land together.
- No Clojure (#5587) or Spock (#5588) semantic law beyond substrate reuse rules.

## Acceptance Examples

1. **substrate-mixed-gradle**: A Gradle project with `src/main/java`,
   `src/main/kotlin`, and `src/test/scala` yields one substrate document with
   three source sets and separate language evidence documents per changed file;
   classpath candidates list `testCompileClasspath` without reach edges.
2. **substrate-generated-kapt**: KAPT output under `build/generated/source/kapt`
   is tagged `generated_role = annotation_processor` with processor coordinates
   when declared; probes on generated lines require `source_provenance =
   generated` and refuse promotion without linkage.
3. **negative-classpath-not-reach**: A test source set listing a production class
   on its classpath MUST NOT create a related-test edge without an evidenced call
   path and activation under #5542.
4. **java-overload-negative**: Two `void process(int)` / `void process(String)`
   overloads changed on one parameter type MUST NOT credit the other overload
   without signature match; unresolved set names `jvm_overload_set_unresolved`.
5. **kotlin-extension-negative**: An extension call `receiver.foo()` MUST NOT
   attribute behavior to a member `foo` on the receiver type without extension
   owner identity.
6. **scala-implicit-negative**: A call requiring an implicit/given instance MUST
   NOT credit a def that merely shares a name in another scope.
7. **test-parameterized-activation**: A JUnit 5 `@ParameterizedTest` with
   `@ValueSource` registers cases separately; oracle credit requires an activated
   case key; disabled parameterized rows stay ineligible.
8. **test-dynamic-generated**: JUnit 5 `@TestFactory` or dynamic tests keep
   `activation = deferred` until a statically known case exists; no oracle credit
   from factory registration alone.
9. **kotlin-coroutine-limit**: A `suspend fun` changed inside `runBlocking` tests
   names `jvm_coroutine_or_async_unresolved` when resume points are not evidenced.
10. **scala-effect-limit**: An `IO`/`Future` effect test without a static oracle
    on the effect boundary names `jvm_coroutine_or_async_unresolved` or a more
    specific limitation declared in the language slice.
11. **cross-language-positive**: Java production code called from Kotlin tests
    links only with a #5559 registered edge carrying JVM method descriptors for
    both sides.
12. **cross-language-negative**: Same simple name in Java and Kotlin without an
    edge remains unrelated despite shared module classpath.
13. **clojure-substrate-only**: A Clojure namespace in the same sbt module may
    read module/classpath facts but MUST NOT appear in Java/Kotlin/Scala capability
    rows or owner namespaces.
14. **spock-substrate-only**: Spock specifications may consume test module facts;
    they MUST NOT count toward Java JUnit/TestNG framework rows.

## Test Mapping

See `.ripr/traceability.toml` behavior `RIPR-SPEC-0394`. Executable tests land
with analysis slices (#5648–#5655) and judged corpora (#5659). Until then,
`fixtures/*/verdict-corpus/spec-scopes/RIPR-SPEC-0394.toml` marks the spec out
of runtime mutant corpora.

## Implementation Mapping

- Substrate modeling: #5649
- Java owners/probes: #5650; Java tests: #5653
- Kotlin owners/probes: #5651; Kotlin tests: #5654
- Scala owners/probes: #5652; Scala tests: #5655
- Relations and limits emission: #5656
- Repo mode completeness: #5657
- Output projection: #5658
- Support evaluation: #5659
- ADR for fact substrate boundaries: #5647

## Metrics

- `jvm_evidence_contract_registered`
- `jvm_capability_cell_index_complete`

## Failure Modes

- Treating substrate classpath facts as reach or oracle eligibility.
- Collapsing Java, Kotlin, and Scala into one owner namespace or support row.
- Promoting preview JVM findings to gate or badge eligibility without promotion
  policy and proof (#5648–#5659).
