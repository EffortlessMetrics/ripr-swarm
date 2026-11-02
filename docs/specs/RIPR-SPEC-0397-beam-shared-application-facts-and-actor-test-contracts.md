# RIPR-SPEC-0397: BEAM Shared Application Facts and Elixir/Erlang Actor-Test Contracts

Status: proposed

Owner: language-adapter / beam

Created: 2026-11-02

Linked proposal:

- None yet (BEAM track under programme #5582)

Linked ADRs:

- Planned: #5876 (BEAM language/project/compiler fact substrates)

Linked plan:

- Epic #5594 (Elixir and Erlang preview evidence for actor and property tests)

Linked issues:

- [#5875](https://github.com/EffortlessMetrics/ripr-swarm/issues/5875) — this spec leaf
- [#5594](https://github.com/EffortlessMetrics/ripr-swarm/issues/5594) — parent epic
- Shared architecture consumed by reference (not implemented here):
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

- None. This document defines preview contracts only. It does not enable BEAM
  adapters, promote Elixir or Erlang support, change default `ripr.toml`
  language lists, or alter public badge or gate posture. Support remains
  governed by [support tiers](../status/SUPPORT_TIERS.md) and the capability
  registry (#5531).

Policy impact:

- Register this spec in `policy/doc-artifacts.d/RIPR-SPEC-0397.toml`.
- Register traceability in `.ripr/traceability.d/beam-spec-0397.toml`.
- Register spec-index fragment `docs/spec-index.d/RIPR-SPEC-0397.md`.
- Register verdict-corpus spec scopes (`scope = "out"`) under each language
  corpus until judged BEAM corpora exist (#5885).
- No new crate feature defaults, parser dependency, runtime executor, or
  support-tier row is introduced by this spec alone.

## Problem

The BEAM ecosystem couples two surface languages (Elixir and Erlang), two build
systems (Mix and rebar3), OTP applications, process/message concurrency, and
several test frameworks (ExUnit, EUnit, Common Test, StreamData, PropEr).
Actor behavior, supervision, and temporal effects cannot be honestly reduced to
“a test calls a function and asserts a return value.”

Without a normative contract that separates shared project facts from independent
language and framework cells, early BEAM work will either duplicate
single-language assumptions or collapse send/receive/supervision into one
undifferentiated “process” credit. Module names, application names, and
framework registration must not mint reach, observation, or `exposed`
promotions.

This spec is the first epic leaf (#5875). It records what passive analysis may
claim before parser, routing, adapter, or corpus implementation (#5876–#5886).

## Behavior

### Authority layering

Analysis obeys three layers. Lower layers must not strengthen upper-layer
semantics.

```text
BeamApplicationFactsV1          shared Mix/rebar3/OTP/project substrate
  -> ElixirLanguageFactsV1      Elixir-only owners, probes, limits
  -> ErlangLanguageFactsV1      Erlang-only owners, probes, limits
       -> FrameworkFact cells    ExUnit | EUnit | Common Test | StreamData | PropEr
            -> ActorTemporalFactsV1   distinct process/message/supervision/temporal facts
                 -> TestActivationFactV1 / OracleEligibilityV1 (#5542)
                      -> shared witness + output projection (#5532, RIPR-SPEC-0026)
```

Shared application facts answer “what project context was selected and with what
currentness?” Language cells answer “what changed owner/probe exists in this
language surface?” Framework cells answer “what test/property registration and
assertion shapes exist?” Actor/temporal facts answer “what concurrency-shaped
evidence is syntactically present?” None of these layers prove runtime schedules,
mailbox order, distributed behavior, or recovery correctness.

### Shared BEAM application substrate (`BeamApplicationFactsV1`)

One portable fact bundle describes the selected BEAM workspace root. It is
language-neutral and must not emit Elixir- or Erlang-specific probe owners,
reach edges, or oracle credit.

Required identity and currentness fields:

| Field group | Contract |
| --- | --- |
| Selected root | Exactly one repo-relative selected root per analysis invocation; relocatable IDs exclude host paths, temp dirs, and drive letters. |
| Build tool | `mix` or `rebar3` (or `unknown_build_tool` limitation). |
| Applications | OTP application names, umbrella child apps, and rel/app metadata as declared in static project files only. |
| Modules | Module atom names and defining source paths as statically discovered; membership in an application is context, not reach. |
| Source roles | Closed roles aligned with #5541 / contextual composition: `production`, `test`, `generated`, `vendor`, `verification_only`, plus `role_unknown` when composition is incomplete. |
| Dependencies | Hex/npm-style lock entries and app deps as **context**; dependency presence does not establish call reach. |
| Compiler/platform | Declared OTP/Elixir/Erlang version constraints and platform tuples from static config; not proof that a toolchain ran. |
| Movement | Content digests and span identities for changed files; byte-stable IDs per RIPR-SPEC-0026 relocation rules. |

Evidence law for the substrate:

- Application or module membership, umbrella nesting, or “same OTP app” co-location
  **must not** establish static reach to a changed sink.
- Shared facts may justify file routing (which adapter may parse a path) but must
  not substitute for language probe attachment.
- When static project graphs conflict (umbrella vs app vs included path), emit a
  typed `beam_project_graph_unresolved` limitation; do not guess a single parent.

Passive analysis **must not** execute `mix`, `rebar3`, `elixir`, `erl`, BEAM
nodes, test suites, or property runs.

### Independent language wire identities

Elixir and Erlang are distinct routed languages:

| Wire `language` | Cargo feature (planned) | Adapter owner (planned) |
| --- | --- | --- |
| `elixir` | `lang-elixir` | `ElixirAdapter` |
| `erlang` | `lang-erlang` | `ErlangAdapter` |

Rules:

- Each language cell carries its own `language_status = preview` until a support
  adjudication (#5886) promotes it under #5531.
- A finding, probe, related test, or limitation on one language **must not**
  borrow capability state from the other.
- Cross-language call edges (Elixir calling Erlang modules and vice versa) require
  a registered positive edge under [RIPR-SPEC-0378](RIPR-SPEC-0378-cross-language-edge-registry-v1.md)
  and #5559 provenance. Name coincidence alone is unrelated.
- NIF and port/native boundaries use boundary families from RIPR-SPEC-0378;
  static analysis records registration and typed limits only.

Stable owner identifiers follow RIPR-SPEC-0026 path qualification:

```text
elixir:<normalized/path.ex>::<qualified_owner>
erlang:<normalized/path.erl>::<module>:<function>/<arity>
```

Macro-generated owners remain producer-limited; expansion is not simulated beyond
explicitly bounded static shapes declared in later adapter specs (#5878, #5879).

### Independent framework and property capability cells

Framework semantics are separate capability cells. IDs use the #5531 shape
`language/framework/mode/family`. Until implementation lands, every cell below
is **`not_proven`** with tier ceiling **`preview`** in the registry projection;
this spec defines the obligation set only.

| Cell ID | Purpose |
| --- | --- |
| `beam/shared/diff/application_facts` | Mix/rebar3/OTP/project substrate only |
| `beam/shared/repo/application_facts` | Repository-mode substrate indexing (#5883) |
| `elixir/any/diff/static_facts` | Elixir owners/probes without a test framework |
| `elixir/exunit/diff/test_facts` | ExUnit case/test/setup/tag/filter/async registration |
| `elixir/streamdata/diff/property_facts` | StreamData generators/properties (registration vs execution separated) |
| `erlang/any/diff/static_facts` | Erlang owners/probes without a test framework |
| `erlang/eunit/diff/test_facts` | EUnit tests/generators/macros |
| `erlang/common_test/diff/test_facts` | Common Test suite/group/case/config/timetrap/skip |
| `erlang/proper/diff/property_facts` | PropEr properties and generators |

Product manifest claims (`metrics/capabilities.toml`) must not imply a cell is
implemented until the registry row exists and proofs land (#5886). Shared
`beam/shared/*` cells must not elevate Elixir/Erlang oracle strength.

### Test activation and oracle eligibility (#5542)

Framework producers parse framework-specific skip/async/tag/timetrap/property
registration syntax. Classification and witnesses consume only the shared
activation and eligibility DTOs from #5542:

```text
TestActivationFactV1  -> registration, selection, activation, expected-outcome mode
OracleEligibilityV1   -> eligible_for_reach | activation | observation | discrimination | repair
```

Law:

- No framework cell may classify `exposed` or strong observation unless
  `OracleEligibilityV1` reports eligibility with an explicit basis.
- `async`, `moduletag`, CT groups, property wrappers, and pending/todo shapes map
  to closed activation vocabulary (`inactive_skip`, `filtered_or_not_selected`,
  `parameterized_or_generated`, `framework_registration_only`, …) — never
  silently `active`.
- Property and actor tests distinguish **registration** from **concrete active
  execution**; registration alone is not execution proof.

Renderers must not infer eligibility from decorator prose (#5542 evidence law).

### Actor, message, supervision, and temporal facts

`ActorTemporalFactsV1` uses a closed fact kind set. Each kind is recorded
separately; merging kinds is forbidden in classification input.

| Fact kind | Meaning (static, not runtime proof) |
| --- | --- |
| `process_startup` | Syntax/spawn pattern suggests a process start |
| `process_registration` | `register` / name registration pattern |
| `process_identity` | PID or registered name reference (literal or binding) |
| `message_send` | `send`, `!`, or equivalent send pattern |
| `message_receive` | `receive`, selective receive, or mailbox match pattern |
| `message_mailbox_pattern` | Mailbox filter/guard on receive |
| `call_cast_reply` | GenServer/call/cast/reply surface |
| `link_monitor` | `link`, `monitor`, `trap_exit` |
| `process_exit` | `exit`, normal/abnormal exit pattern |
| `process_crash` | `throw`, `error`, `exit` crash semantics in test/oracle context |
| `supervision_start` | Supervisor child spec or start link |
| `supervision_restart` | Restart intensity / `:permanent` / `:temporary` static shape |
| `state_transition` | Callback clause or explicit state record update |
| `timeout` | `after` timeout or `:timeout` tuple in receive/call |
| `ordering_claim` | Assertion or comment claiming order (always weak unless independently established) |

Core actor invariant (epic #5594):

```text
message_send  +  message_receive/assertion  +  discriminator  ->  may establish observation credit
message_send alone                                           ->  effect only, not observed behavior
process_startup alone                                        ->  not observed behavior
```

`message_send` may attach as propagation or side-effect context. It must not
upgrade to `exposed` unless a related test or oracle on the receiving/asserting
process establishes a matching discriminator under #5542 eligibility.

Supervision restart, exit reason, and state transition facts remain distinct
rows in witness output (#5532); collapsing them into one “OTP” bucket is forbidden.

### Typed limitations (fail closed)

When static analysis cannot honestly resolve BEAM-specific shapes, producers emit
`static_limit_kind` (or successor BEAM-specific kinds registered alongside
RIPR-SPEC-0026) including at least:

| Limit kind | Typical trigger |
| --- | --- |
| `beam_macro_unexpanded` | Macro/parse transform body not expanded |
| `beam_protocol_or_behaviour_unresolved` | Protocol/behaviour/callback target unknown |
| `beam_dynamic_apply` | `apply/3`, MFA variables, or computed fun |
| `beam_runtime_registration` | Dynamic `register/2`, ETS `:named_table`, process dictionary |
| `beam_code_hot_load` | Hot code upgrade attributes or `:code.load` patterns |
| `beam_mailbox_order_unknown` | Ordering or selective receive not pinned by test |
| `beam_async_test_uncertain` | ExUnit async / CT parallel / property async without eligibility |
| `beam_distribution_unknown` | Node names, `:rpc`, cluster config |
| `beam_native_boundary` | NIF/port/`c:port`/`Port.open` without #5559 edge |
| `beam_generated_source` | `.ex` under `/_build`, `rebar3` `_build`, or tagged generated role |
| `beam_supervision_topology_unresolved` | Dynamic child specs, `start_link` MFA variables |

Limitations are typed uncertainty, not silent `no_static_path` coercion.

### Cross-language and native edges

- Elixir↔Erlang calls, NIFs, and ports require positive registered edges (#5559,
  RIPR-SPEC-0378) with explicit producer basis.
- Module or application name equality across languages without an edge is
  **unrelated**.
- Shared application facts may list both languages in one repo but must not imply
  cross-language reach.

### Shared witness and output model

BEAM findings project through the same language-neutral evidence spine as
RIPR-SPEC-0026 and shared witness adapters (#5532, RIPR-SPEC-0204). No
BEAM-only public schema fork, CLI, or planner (#5162 remains the verification
route authority).

Human, JSON, SARIF, LSP, MCP, and agent surfaces inherit additive metadata only
(`language`, `language_status`, `framework`, actor fact kinds, activation blockers).

## Required Evidence

This spec slice is documentation-only. Future implementation must provide:

- Fixture-backed passive facts for Mix and rebar3 umbrella layouts (#6201).
- Independent Elixir and Erlang owner/probe corpora (#5885, #6219).
- Activation projection fixtures per framework cell (#6203–#6207, #6209).
- Actor/message/supervision discriminating controls (#6211, #6212).
- Cross-language/NIF/port negative controls with and without RIPR-SPEC-0378 edges.
- Capability registry rows matching the cell table with `not_proven` until #5886
  adjudication.

## Non-Goals

- No BEAM/Mix/rebar3/compiler/test/node/NIF/port execution during analysis.
- No parser, tokenizer, or adapter implementation (later #5877–#5879).
- No default enablement of `lang-elixir` / `lang-erlang` or `ripr.toml` language
  auto-detection.
- No static proof of temporal or concurrent correctness.
- No universal macro, parse-transform, or supervision-topology resolution.
- No generated tests, edits, support-tier promotion, Hex publication, or release
  action.
- No implementation of #5526/#5531/#5532/#5541/#5542/#5546/#5559/#5560 beyond
  citations; those issues remain their own delivery lanes.

## Acceptance Examples

1. **Independent cells.** An ExUnit async case and an EUnit test in the same repo
   produce findings tagged with distinct `language` and `framework` cells; neither
   borrows the other's capability state.
2. **Activation before oracle.** A skipped ExUnit test remains a related pointer
   with `test_activation_unestablished` / #5542 `inactive_skip`; it cannot yield
   `exposed`.
3. **Send vs receive.** A changed `send(pid, msg)` line with no receiving assertion
   may record `message_send` but classification stays below `exposed` until a
   eligible receive/assertion discriminator exists on the asserting process.
4. **Distinct supervision facts.** A supervisor restart change records
   `supervision_restart` separately from `process_exit`; witness rows do not merge
   them.
5. **Typed macro limit.** A probe inside an unexpanded macro emits
   `beam_macro_unexpanded`; it does not silently classify as `no_static_path`.
6. **Shared facts do not strengthen semantics.** Knowing `my_app` contains modules
   `A` and `B` does not create reach from a change in `A` to tests of `B`.
7. **Cross-language name collision.** Elixir module `Foo` and Erlang module `foo`
   without a registered edge produce unrelated subjects.
8. **Shared witness.** Elixir and Erlang findings appear in the same JSON finding
   shape with additive BEAM fields; no second schema version fork.

## Test Mapping

Planned tests:

- `beam_application_facts_do_not_establish_reach`
- `elixir_and_erlang_capability_cells_are_independent`
- `exunit_skip_projects_to_inactive_activation`
- `message_send_without_receive_is_not_observed`
- `supervision_restart_and_exit_remain_distinct_witness_rows`
- `beam_macro_limit_is_typed_not_silent`
- `cross_language_module_name_without_edge_is_unrelated`
- `shared_substrate_finding_uses_neutral_language_metadata`

Implementation slices #5877–#5885 own executable proof; this spec registers the
obligations and corpus scopes only.

## Implementation Mapping

| Slice | Issue | Delivers |
| --- | --- | --- |
| ADR substrates | #5876 | Parser/compiler fact-store decisions |
| Routing + substrate | #5877, #6200, #6201 | `BeamApplicationFactsV1` producers |
| Elixir adapter | #5878, #6202–#6204 | Elixir owners + ExUnit |
| Erlang adapter | #5879, #6205–#6207 | Erlang owners + EUnit/CT |
| Property cells | #5880, #6209–#6210 | StreamData/PropEr |
| Actor model | #5881, #6211–#6212 | `ActorTemporalFactsV1` |
| Relations/limits | #5882, #6213–#6214 | Edges + actionability limits |
| Repo mode | #5883, #6215–#6216 | Bounded multi-app completeness |
| Output parity | #5884, #6217–#6218 | Projections |
| Corpora/support | #5885–#5886, #6219–#6221 | Judged corpora + tier packets |

Activation DTO consumption follows #6387 → #6389 → #6391 sequencing under #5542.

## CI Proof

Documentation gates for this slice:

```bash
cargo xtask check-spec-format
cargo xtask check-spec-numbering
cargo xtask check-doc-artifacts
cargo xtask check-doc-index
cargo xtask check-traceability
cargo xtask check-static-language
cargo fmt --check
cargo xtask precommit
```

Hosted merge qualification remains separate; green policy gates do not imply BEAM
adapters are compiled or enabled.

## Metrics

- `spec_format_pass_rate` — spec template and section compliance.
- `traceability_registration_completeness` — behavior row, doc artifact, and spec
  scopes present for RIPR-SPEC-0397.

Promotion metrics for BEAM preview accuracy belong to #5885 corpora, not this spec.

## Failure Modes

- Treating Mix dependency lists as call graphs → false reach; prevented by substrate
  evidence law.
- Crediting `send` as observation → false `exposed`; prevented by actor invariant.
- Merging async registration with executed cases → false activation; prevented by
  #5542 projection requirement.
- Registering only one BEAM language cell → hidden cross-language strengthening;
  prevented by independent cell table.
- Implementing BEAM-only output schema → breaks #5532 parity; forbidden by this spec.
