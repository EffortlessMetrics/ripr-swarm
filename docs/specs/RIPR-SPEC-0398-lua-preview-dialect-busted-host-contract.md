# RIPR-SPEC-0398: Lua preview dialect, Busted, table, metatable, host, and native boundaries

Status: proposed

Owner: product / swarm

Created: 2026-11-02

Linked proposal:

- [RIPR-PROP-0001: Multi-Language Adapter Preview](../proposals/RIPR-PROP-0001-multi-language-adapter-preview.md)

Linked ADRs:

- None yet. Parser, rockspec, and host fact substrates are scoped to #5900.

Linked plan:

- Epic [#5597](https://github.com/EffortlessMetrics/ripr-swarm/issues/5597) under programme
  [#5582](https://github.com/EffortlessMetrics/ripr-swarm/issues/5582).

Linked issues:

- [#5899](https://github.com/EffortlessMetrics/ripr-swarm/issues/5899) — governing spec leaf
- [#5597](https://github.com/EffortlessMetrics/ripr-swarm/issues/5597) — Lua preview epic
- Shared architecture consumed by reference only (not implemented here):
  [#5526](https://github.com/EffortlessMetrics/ripr-swarm/issues/5526),
  [#5531](https://github.com/EffortlessMetrics/ripr-swarm/issues/5531),
  [#5532](https://github.com/EffortlessMetrics/ripr-swarm/issues/5532),
  [#5541](https://github.com/EffortlessMetrics/ripr-swarm/issues/5541),
  [#5542](https://github.com/EffortlessMetrics/ripr-swarm/issues/5542),
  [#5546](https://github.com/EffortlessMetrics/ripr-swarm/issues/5546),
  [#5559](https://github.com/EffortlessMetrics/ripr-swarm/issues/5559),
  [#5560](https://github.com/EffortlessMetrics/ripr-swarm/issues/5560)

Linked PRs:

- None yet

Support-tier impact:

- **No tier change.** Lua remains absent from default builds and support tiers until
  a later adjudication slice (#5911). This spec defines the preview contract only.
  Findings use `language = "lua"` and `language_status = "preview"`. Nothing here
  enables Lua in `[languages] enabled`, adds a Cargo feature, promotes gate/badge/RIPR
  Zero eligibility, or claims LuaRocks publication readiness.
- Claim boundaries remain governed by [support tiers](../status/SUPPORT_TIERS.md).

Policy impact:

- Register in `policy/doc-artifacts.d/RIPR-SPEC-0398.toml`, `docs/spec-index.d/RIPR-SPEC-0398.md`,
  and `.ripr/traceability.d/5899-lua-preview-contract.toml`.
- No new crates, binaries, dependencies, parsers, runtime executors, or LSP servers are
  introduced by this spec document.

## Problem

Lua is widely embedded. Tables, metatables, globals, dynamic `require`/`load`, coroutines,
and host-injected APIs make source-local test-to-production inference unusually unsafe.
Before any parser, router, or analyzer lands (#5900 onward), RIPR needs one accepted
preview contract that states:

- which dialect/runtime and repo-mode facts are independent capability cells;
- how module/table owners, colon/dot calls, and metamethod dispatch preserve receiver
  provenance;
- how Busted registers tests and emits bounded oracle and interaction facts;
- how spies, stubs, and mocks differ from value, state, and output observations;
- which dynamic, global, metatable, host, native, and C-module edges may appear only
  through registered cross-language evidence (#5559, [RIPR-SPEC-0378](RIPR-SPEC-0378-cross-language-edge-registry-v1.md));
- which typed static limits apply when syntax-first analysis cannot establish reach or
  observation.

Without this contract, later Lua slices can invent incompatible owner identity, treat
table keys or `require` strings as reach, or merge embedded-host/vendor roles with ordinary
repair targets.

## Behavior

### Shared architecture (by reference)

This spec consumes the shared multi-language spine without redefining it:

- Language-neutral adapter boundary: [RIPR-SPEC-0026](RIPR-SPEC-0026-language-adapter-contract.md)
- Capability reporting vocabulary: #5531 (via `ripr capabilities --json`)
- Test activation and oracle eligibility gates: #5542 (Busted activation maps through
  this authority before an oracle may credit a changed sink)
- Host/native and cross-language positive edges: #5559 and RIPR-SPEC-0378 (no positive
  host, FFI, or C-module reach without a registered producer fact)
- Shared witness and output schemas: existing check JSON, human, SARIF, GitHub, LSP,
  agent packet, and diagnostic witness fields remain the projection surfaces; Lua adds
  only bounded fact vocabulary and limitation kinds, not a schema fork.

Passive analysis must not execute `lua`, `luajit`, `luarocks`, `busted`, or load C modules.

### Wire identity, routing, and opt-in posture

| Field | Contract |
| --- | --- |
| `language` wire id | `lua` (exact string) |
| Routed extensions | `.lua` |
| Default enablement | **Off.** Lua runs only when `[languages] enabled` includes `lua` and the binary is built with a future `lang-lua` feature (to be introduced in implementation slices). |
| `language_status` | `preview` for every Lua finding |
| Actionability default | Preview advisory; `repair_packet_ready` stays fail-closed until a later slice explicitly satisfies the shared actionability contract (#5546) under preview rules |

Project detection without `ripr.toml` must **not** silently enable Lua. An explicit
`ripr.toml` remains authoritative.

### Independent capability cells

Lua evidence is reported through **separate capability cells**. One cell changing state
must not imply another cell is `proven`, `available`, or gate-eligible. At minimum the
registry must be able to distinguish:

| Axis | Example cell id pattern | Reports |
| --- | --- | --- |
| Dialect / runtime | `lua/5.1/diff/static_facts`, `lua/5.4/diff/static_facts`, `lua/luajit/diff/static_facts` | Which Lua dialect or LuaJIT profile was selected for the run |
| Package / rockspec | `lua/rockspec/repo/package_discovery` | Standalone luarocks package layout vs unknown |
| Repo mode | `lua/standalone/repo/static_facts`, `lua/embedded/host/static_facts` | Ordinary package tree vs embedded-host/vendor disposition |
| Test framework | `lua/busted/diff/test_framework` | Busted registration and selector coverage |
| Table / metatable owners | `lua/table/diff/owner_facts` | Table member and metamethod owner extraction |
| Interaction / test doubles | `lua/busted/diff/interaction_facts` | Spy/stub/mock call observations (caller-side only) |
| Relations | `lua/diff/related_tests` | Exact helper/table/global relations where bounded |
| Host / native | `lua/host/diff/native_edges` | Registered cross-language edges only |
| Actionability | `lua/diff/repair_routing` | Preview repair routing when explicitly implemented |
| Verification | `lua/verification/external` | External receipt hooks; not passive analysis |

Cells are **not** merged into one “Lua works” claim. Partial, capped, dialect-limited,
or embedded-host incomplete runs must surface `limited` or `unavailable` disposition on
the affected cells (#5597 acceptance).

### Dialect and runtime context

The adapter retains explicit dialect/runtime context rather than claiming universal
Lua grammar parity:

- Supported dialect labels: `lua51`, `lua52`, `lua53`, `lua54`, `luajit`, `unknown`.
- Dialect may be declared by rockspec fields, toolchain markers, or repo config when
  implementation lands; when unknown, findings carry `dialect = unknown` and dialect-specific
  oracle rules fail closed.
- LuaJIT-only syntax without a declared `luajit` context is `unsupported_syntax` or
  `dialect_feature_unresolved`, not silent coercion to another dialect.
- Dialect differences (vararg semantics, integer division, environment models, deprecated
  library surfaces) must not be unified into one behavior class.

### Source roles and repository mode

Each analyzed file carries a **source role** used for selection, non-claims, and repair
target policy:

| Role | Meaning |
| --- | --- |
| `rockspec` | Package metadata; not a behavioral owner surface |
| `package_module` | `lib/` or declared module root module |
| `package_script` | Executable entry declared by the package |
| `test` | Busted spec, example, or declared test root |
| `generated` | Generated Lua; disclosed, not a default repair target |
| `vendor` | Vendored third-party tree; not a default repair target |
| `embedded_host` | Host application glue loading or configuring Lua |
| `top_level_script` | Non-module script executed directly |

Standalone package corpora and embedded-host corpora are judged separately (#5597). Vendor,
generated, and embedded-host roles must not silently become ordinary project repair targets.

### Owner facts, tables, colon/dot, and probe identity

Owners the adapter must recognize when syntax-first extraction is sufficient:

- module-level `function` and `local function`
- functions assigned into table fields (named or anonymous)
- table “method” definitions using `function T:method()` and `function T.method()`
- constructor-like factories that return tables with function members (owner is the
  factory function and/or the table literal identity as declared by the implementation slice)
- metamethod declarations `function T:__index()` / `__newindex` / `__call` / etc.
- top-level script statements that participate in changed behavior

**Identity rules (load-bearing):**

```text
colon call T:foo()  → receiver table identity + method key + implicit self parameter slot
dot call  T.foo()   → table identity + field key; no implicit self unless syntax shows one
anonymous function assigned to T[k] → stable probe id for that table member, not k alone
metamethod            → metamethod owner distinct from arbitrary table field owners
```

Stable `probe.owner` shape (language-qualified, path-qualified):

```text
lua:<normalized/path.lua>::<qualified_owner>
```

Examples:

```text
lua:src/pricing.lua::apply_discount
lua:src/models.lua::Cart.apply_discount
lua:src/models.lua::Cart:__index
lua:init.lua::<module>
```

**Non-claims:** table/member names, string keys, `require "..."` module names, global
names, spy/stub target names, and host API identifiers **cannot** establish reach,
observation, or owner equality by themselves. Token coincidence on the wrong receiver
remains `weakly_exposed` or a named limitation, not `exposed`.

### Module loading and `require` boundaries

Static facts may record **syntax-visible** `require`, `package.loaded`, and `load`/`loadfile`
call sites. They must not simulate a runtime module graph.

- Resolved `require` to a same-repo file is a bounded lexical fact when the implementation
  slice can tie the string to one normalized path without executing loaders.
- Dynamic `require` (variable module name, computed string, `load` on buffers, `dofile` on
  non-literal paths) stops at `dynamic_module_load_unresolved`.
- C modules (`require "cjson"`, `luaopen_*`) never gain production reach from the module
  name alone; positive native edges require #5559 registry evidence with family
  `native_addon` or `ffi_binding` as applicable.

### Busted activation (#5542)

Busted is the first test framework cell. Test registration facts are separate from oracle
facts and from interaction facts.

**Registration surfaces (syntax-first):**

- `describe`, `context`, `it`, `specify`, `test` blocks
- `setup`, `teardown`, `before_each`, `after_each`, `lazy_setup`, `pending`
- tag declarations (`@tag`, `#tag`, tag filters on blocks when syntactically visible)
- parameter/helper-generated examples only when the generator call is syntactically bounded;
  otherwise `generated_test_registration_unresolved`

**Activation mapping:** a test block is **active** for oracle eligibility only when #5542
rules pass for Busted (not pending, not excluded by a visible skip/filter the static contract
evaluates, not outside a selected tag filter when filter context is known). Pending,
excluded, or tag-filtered blocks retain related-test pointers where appropriate but carry
`test_activation_unestablished` and emit no oracle credit.

**Selector limits:** tag filters, random seeds, custom Busted options, and runtime-only
registration hooks are not evaluated. When selection is unknown, the run fails closed on
oracle credit for affected blocks while still disclosing registration structure.

**Verify command shape (when evidence-backed):** only when rockspec/package scripts or
declared dev dependencies support it; never invented from a file extension alone:

```bash
busted --verbose <path/to/spec.lua>
```

### Assertion and oracle facts (value/state/output)

Oracle facts describe **what a test assertion observes** about a changed sink. They are
distinct from interaction facts (below) and from global/table mutation observations.

Bounded Busted assertion families include, when syntax resolves the callee and subject:

| Family | Static intent |
| --- | --- |
| equality / truth | `assert.are.equal`, `assert.is_true`, `assert.is_false`, `assert.same` |
| predicates | `assert.matches`, `assert.truthy`, type checks |
| errors | `assert.has_error`, `assert.error`, `assert.errors` |
| collections / tables | table length, key presence, shallow field pins when literal |
| output | captured stdout/stderr helpers when call shape is visible |
| state / globals | `_G` or upvalue mutation only when assignment target is syntactically tied to the changed owner |
| coroutine | `coroutine.create`/`resume` observations only when scheduling is syntactically bounded |

Custom assertion helpers, opaque `assert.*` extensions, and host-provided matchers are
`opaque_custom_assertion_helper` unless a registered helper relation ties call shape to
observed sink.

Global mutation, environment tables, and debug-library reads (`debug.getupvalue`, etc.)
do not establish observation unless the static contract explicitly models the binding;
otherwise `global_mutation_unresolved` or `debug_api_unresolved`.

### Spies, stubs, mocks, and interactions

Busted test doubles (`spy`, `stub`, `mock`, `mock.revert`, `snapshot`) produce **interaction
facts**, not production oracle credit on the doubled implementation.

Rules:

- Verifying a stub/spy records **caller dispatch** and argument shapes visible at the call
  site; it does not grip unrelated production behavior behind the double.
- Replacing a table member or global with a stub means production code under the real
  implementation is **not** observed by that test path; relation kind reflects test-double
  indirection and caps classification at `reachable_unrevealed` unless an independent
  non-double oracle exists.
- Mock objects with programmed returns credit only the programmed expectation visible in the
  spec file, not arbitrary methods on the real module.
- `mock.revert` and monkeypatched `_G` entries trigger `global_mutation_unresolved` or
  `monkeypatch_unresolved` for production reach.

Interaction facts must identify receiver/table, member key, and call site span separately
from value assertions on return data.

### Metatable dispatch and coroutines

When `__index`, `__newindex`, `__call`, or other metamethods route control flow without a
statically resolved target, classification stops at `metatable_dispatch_unresolved`. Partial
`__index` tables with literal function values may credit only those literal targets; computed
metamethods and chained metatables fail closed.

Coroutines (`coroutine.wrap`, `yield`, host-driven resume) use `coroutine_scheduling_unresolved`
when execution order or completion is not syntactically bounded. Coroutine observations do not
imply the wrapped function body was fully exercised.

### Host injection and native/C-module edges

Host applications may inject APIs into the Lua environment. Such APIs are **not** inferred
from names in source text.

Positive claims that production code reached through a host API, C module, or native addon
require a registered edge in the cross-language registry (RIPR-SPEC-0378) with producer
provenance per #5559. Unregistered host callbacks, game-engine bindings, nginx `ngx.*`,
Redis `redis.call`, and similar surfaces remain `host_injected_api_unresolved`.

Embedded runtime state (registry, per-request contexts, opaque userdata) is not a repair
target unless role `embedded_host` disclosure explicitly scopes analysis.

### Static limitation vocabulary (Lua extensions)

When syntax-first analysis cannot classify, the adapter emits explicit `static_limit_kind`
values (additive to RIPR-SPEC-0026 where shared kinds apply):

| Kind | When |
| --- | --- |
| `dynamic_dispatch` | Callable value not resolved to one owner |
| `metatable_dispatch_unresolved` | Metamethod or `__index` chain not resolved |
| `dynamic_module_load_unresolved` | Non-literal `require`/`load` |
| `global_mutation_unresolved` | `_G` or environment table mutation not tied to owner |
| `debug_api_unresolved` | Debug library indirection |
| `generated_function_unresolved` | Generated or macro-like function bodies |
| `monkeypatch_unresolved` | Runtime replacement of globals or table members |
| `coroutine_scheduling_unresolved` | Coroutine order/completion unknown |
| `dialect_feature_unresolved` | Dialect-specific construct without declared dialect |
| `host_injected_api_unresolved` | Host/native surface without registered edge |
| `opaque_custom_assertion_helper` | Unmodeled assertion helper |
| `unsupported_syntax` | Parse failure |
| `mocked_module` | Production path replaced by test double for that relation |

Limitations map to exposure classes per shared rules: they are not silent `no_static_path`
coercions unless the shared classifier says so, and they never emit actionable repair packets
while preview advisory posture holds.

### Output and witness projection

Lua findings reuse existing output contracts:

- `check.json` findings carry `language`, `language_status`, `probe.owner`, optional
  `owner_kind`, `static_limit_kind`, and bounded related-test/oracle metadata.
- Human, SARIF, GitHub, LSP diagnostic/hover, agent context packets, and diagnostic witness
  rows project the same facts without forking schema versions.
- Preview disclosure lines follow [RIPR-SPEC-0082](RIPR-SPEC-0082-preview-language-disclosure.md).

Partial/capped runs include limitations with recovery text naming the budget, dialect, repo
mode, or host scope that blocked completion.

## Inputs

- `.lua` files routed under opt-in Lua preview configuration.
- Diff spans inside those files.
- Declared dialect/runtime context when present (rockspec, config, toolchain markers).
- Source role classification inputs (path conventions, rockspec declarations, embedded/vendor
  markers) without executing LuaRocks.
- Registered cross-language edge registry documents supplied through the #5559 authority path
  when host/native reach is claimed.

The adapter does not consume runtime Busted results, Lua VM state, or live host callbacks.

## Outputs

- Language-tagged static facts, probes, classifications, and limitations as above.
- Capability cell disposition updates per #5531 when the adapter is implemented.
- Optional bounded `lua_repair_card` / repair packet shapes only in later slices after #5546
  preview actionability rules are satisfied; this spec does not require them.

## Non-Goals

- No Lua/LuaJIT/host/Busted/C-module execution during passive analysis.
- No parser, tree-sitter grammar, router, or Rust adapter implementation in this document.
- No universal host API, metatable, or dynamic loader resolution.
- No generated tests or automated edits.
- No default enablement, support-tier promotion, LuaRocks publication, or release action.
- No new verification planner; later routes consume #5162.

## Required Evidence

**This spec slice (documentation only)** is satisfied when repository gates confirm registration:

- `cargo xtask check-spec-format`
- `cargo xtask check-spec-numbering`
- `cargo xtask check-doc-artifacts`
- `cargo xtask check-traceability`
- `cargo xtask check-static-language`
- `cargo xtask precommit` on the spec/doc delta

**Future implementation slices** must additionally provide:

- Fixture or verdict-corpus rows per acceptance example below.
- Capability registry cells registered and claimed per RIPR-SPEC-0365.
- Cross-language corpus cases for host/native edges per RIPR-SPEC-0378.

## Acceptance Examples

1. **Dialect cells explicit:** A run on a rockspec-declared 5.1 package reports
   `lua/5.1/diff/static_facts` separately from `lua/busted/diff/test_framework`; neither
   cell implies the other is complete.
2. **Colon vs dot:** `obj:send()` and `obj.send()` yield distinct receiver/self facts for the
   same table symbol; swapping forms does not reuse the wrong probe id.
3. **Table member assignment:** `T.helper = function() ... end` receives a stable anonymous-member
   probe id; the string `helper` alone does not establish reach to unrelated tables.
4. **Spy vs assert:** A test with `assert.spy(fn)` call-count expectations credits interaction
   on the spy wrapper only; a literal return-value `assert.are.equal` on a non-doubled call
   remains a separate oracle fact.
5. **Stubbed module:** When `package.loaded` or a stub replaces a module, production
   implementation behind the stub is not `exposed` through stub invocation credit.
6. **Metatable stop:** A call routed only through unresolved `__index` metatable chain receives
   `metatable_dispatch_unresolved`, not `exposed`.
7. **Dynamic require:** `require(modname)` with non-literal `modname` receives
   `dynamic_module_load_unresolved`.
8. **Host API name only:** Source mentioning `ngx.say` without a registered cross-language edge
   receives `host_injected_api_unresolved`.
9. **Registered native edge:** With a valid #5559-backed registry edge from Lua call site to
   native subject, reach may be stated only to the edge’s bounded strength ceiling.
10. **Embedded/vendor roles:** Files under declared vendor/embedded roles are labeled non-targets;
    findings disclose scope instead of suggesting production repair in vendor trees.
11. **Busted pending:** A `pending()` example registers structure but carries
    `test_activation_unestablished` and no oracle.
12. **Partial run:** Hitting a diff or read budget emits explicit limitations; outcome is not
    `complete` with silent omission.

## Test Mapping

See `.ripr/traceability.d/5899-lua-preview-contract.toml` behavior `RIPR-SPEC-0398`.
Implementation tests and fixtures are intentionally deferred until #5901+.

## Implementation Mapping

Future work (out of scope for #5899):

- `analysis/language/lua/` adapter and router (#5901)
- Busted fact extractors (#5903–#5904)
- Output projection (#5909)
- Eval corpus (#5910)

## CI Proof

Documentation slice:

```bash
cargo xtask check-spec-format
cargo xtask check-spec-numbering
cargo xtask check-doc-artifacts
cargo xtask check-traceability
cargo xtask check-static-language
cargo xtask precommit
```

## Metrics

- `lua_preview_contract_spec_proposed` — this document is registered and gates pass.

## Failure Modes

- Treating member/`require`/global/host names as reach or observation.
- Merging spy/stub interaction credit with production oracle credit.
- Collapsing dialect, repo mode, Busted, and host/native cells into one status.
- Emitting verify commands without package/script evidence.
- Claiming host/native reach without #5559 registry provenance.
- Presenting partial embedded-host analysis as a clean complete result.
