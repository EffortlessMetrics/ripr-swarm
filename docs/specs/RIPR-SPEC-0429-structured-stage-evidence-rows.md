# RIPR-SPEC-0429: Structured staged-evidence rows

Status: proposed

Owner: product-output

Created: 2026-11-05

Linked issues:

- [EffortlessMetrics/ripr-swarm#13162](https://github.com/EffortlessMetrics/ripr-swarm/pull/13162) (agent-efficiency study, Nov 2-4: delivered-context cost of prose-only evidence; the merged learnings record)
- [EffortlessMetrics/ripr-swarm#13453](https://github.com/EffortlessMetrics/ripr-swarm/issues/13453) (producer-backed stage test attribution — the explicit non-goal of this slice)
- [EffortlessMetrics/ripr-swarm#12415](https://github.com/EffortlessMetrics/ripr-swarm/issues/12415) (round-11 context: the agent gap-packet stage overclaim; related surface, not this slice's acceptance)

Linked specs:

- [RIPR-SPEC-0001](RIPR-SPEC-0001-static-exposure-loop.md) — evidence-first finding output
- [RIPR-SPEC-0021](RIPR-SPEC-0021-evidence-record.md) — the staged evidence chain (`reach`/`activate`/`propagate`/`observe`/`discriminate`)
- [RIPR-SPEC-0026](RIPR-SPEC-0026-language-adapter-contract.md) — additive finding fields
- [RIPR-SPEC-0242](RIPR-SPEC-0242-canonical-next-action.md) — additive check JSON adapter precedent

Support-tier impact: none (output projection of existing evidence; no classification, route, or gate change); see [support tiers](../status/SUPPORT_TIERS.md).

Policy impact: register in `policy/doc-artifacts.toml` and `.ripr/traceability.toml`.

## Problem

The staged evidence chain has one producer — the finding's `ripr` stages — but
its machine-readable surfaces were incomplete in both directions:

- `ripr check --format json` carried the states under `ripr` and then the same
  chain again as English sentences in the `evidence_path` prose array
  (`"reach yes: …"`, `output/json/report.rs::evidence_path_values`). A script
  that wanted the staged-evidence narrative — stage, state, why a stage is
  capped — had to parse sentences, because no row form tied them together.
- `ripr check --format agent` and `ripr explain --format agent` carried no
  stage states at all: a gap record named the class and the strongest test,
  but an agent reading only the compact surface could not see that reach was
  `unknown` or that discrimination was `no` without opening the full JSON
  document.

The agent-efficiency study (Nov 2-4) measured the cost of this shape: ripr
context delivered 3.69× the vanilla bytes while the needed subset was 26.1% of
it, and its durable rule is "render each fact once" — evidence rendered only
as prose that a consumer must re-parse is a fact rendered in the wrong
alphabet for agents.

## Behavior

The staged-evidence row form is a pure projection of the finding's own
evidence. No classification, ranking or decision is added, and the human
rendering stays prose.

### `evidence_stages` on check JSON findings

Each finding in `ripr check --format json` (and the `json-compact`
minification of the same document) carries an additive `evidence_stages`
array: exactly five rows, in the fixed Evidence-block order
`reach`, `infect`, `propagate`, `observe`, `discriminate`. Row fields:

- `stage` — the `ripr` object's own key for that stage, so a consumer joins
  rows to the typed stage objects by the same token.
- `state`, `confidence` — the stage's own `StageState`/`Confidence` wire
  tokens, byte-identical to the values under `ripr`.
- `basis` — optional, present only when the finding's `confidence_basis`
  (#10700) names this stage, in the shared `{code, summary}` shape. The
  mapping is owned by the domain (`ConfidenceCap::capped_stage`): only a cap
  whose plain meaning names exactly one stage claims that stage's row
  (`no_related_test`→`reach`, `infection_not_established`→`infect`,
  `propagation_not_established`→`propagate`,
  `observation_not_confirmed`→`observe`,
  `discriminator_not_established`→`discriminate`). Finding-, adapter-,
  oracle- and activation-level caps (`preview_stages_not_modeled`,
  `syntax_first_no_type_resolution`, `oracle_below_strong`,
  `boundary_input_not_matched`) claim no row. Rust findings carry no
  `confidence_basis`, so no row carries `basis`.

No row carries test or assertion references. No producer records which
assertion established a stage; the general `related_tests` role is the legacy
decision set, not stage provenance, and a row of that set can carry a
`no_call_path` or `no_assertion` miss while the stage reads `yes`. Projecting
those rows onto a stage would be a wrong actionable signal. Producer-backed,
row-distinct attribution is owned by #13453; until a producer records it,
the rows claim nothing and consumers read the existing `related_tests` and
`assertion_texts` for the test-side facts.

`schema_version` stays `0.2`: the change is additive per the check contract's
bump rule ("nested `finding` … may grow additively within the pinned
version"), the same rule #10700 and #7258 used. `json-terse` codes nothing in
the new field (states and tokens are short shared vocabulary; `basis` entries
ride the document unchanged), and expanding the terse legend reconstructs the
same rows.

### `stages=` compact row on the agent surfaces

`ripr check --format agent` prints one `stages=` line directly after each
finding record (gap, `open=`, and limited records):
`stages=reach:yes,infect:weak,propagate:unknown,observe:yes,discriminate:no`.
`ripr explain --format agent` prints the same line after its one record.
States use the same closed vocabulary as the JSON rows; the line names states
only (basis stays JSON-side), and runs with no records emit no `stages=`
lines.

## Required Evidence

- `crates/ripr/src/output/stage_rows.rs` is the single shared projection
  (rows, JSON emitter, agent line) with unit tests pinning: the five-row
  order and vocabulary; that stage-named basis entries land on their own
  stage row and finding-level caps land on none; that no row references a
  related test whatever its role or miss (the #13453 negative control); and
  the compact agent line.
- `crates/ripr-contract/src/domain/confidence_basis.rs` pins the
  `capped_stage` mapping exhaustively over the closed cap set.
- `fixtures/boundary_gap/expected/check.json` (and every fixture golden
  regenerated in the same additive pass) pins the emitted rows for a real
  producer finding.
- `schemas/ripr/check.schema.json` carries `evidence_stages` as an additive
  optional finding property; `docs/OUTPUT_SCHEMA.md` documents the rows and
  the `stages=` line in both agent grammars.
- `cargo xtask check-output-contracts` stays green with the schema change.

## Non-Goals

- No human-rendering change: the Evidence block, digest line and agent
  terminal lines stay prose.
- No change to `ripr`, `evidence_path`, `related_tests`,
  `confidence_basis` or `assertion_texts` values themselves — the rows
  project them.
- No test or assertion references on any stage row: no producer records
  per-stage attribution, and a renderer must not re-derive classifier
  decisions (`establishes_reach` depends on owner-kind facts the finding does
  not carry; the decision-set role is not provenance). Owned by #13453.
- No schema_version bump, no removal or rename of existing fields.
- No new stage states, cap codes, or exposure semantics.
- Context-packet, evidence-record, repo-exposure and SARIF surfaces keep
  their existing shapes.

## Acceptance Examples

- A `weakly_exposed` Rust finding (the `boundary_gap` fixture) renders:
  `"evidence_stages": [{"stage":"reach","state":"yes","confidence":"medium"},
  {"stage":"infect","state":"weak","confidence":"medium"},
  {"stage":"propagate","state":"yes","confidence":"medium"},
  {"stage":"observe","state":"yes","confidence":"medium"},
  {"stage":"discriminate","state":"yes","confidence":"medium"}]`
  and `--format agent` prints
  `stages=reach:yes,infect:weak,propagate:yes,observe:yes,discriminate:yes`
  after the gap record.
- A TypeScript preview finding capped by `observation_not_confirmed` renders
  that cap's code and summary on the `observe` row only; the
  `boundary_input_not_matched` cap appears on no row because it names no
  single stage.
- A finding whose related tests include a `no_call_path` row while
  `discriminate` reads `yes` (the `rust_slice_table_try_exit` mean-latency
  finding) renders five rows and no `tests` member anywhere.

## Test Mapping

- `crates/ripr/src/output/stage_rows.rs` unit tests (listed in Required
  Evidence).
- `crates/ripr/src/output/human/agent_tests.rs` pins the `stages=` line in
  the check agent grammar and its absence on record-free terminal runs.
- `crates/ripr-contract/src/domain/confidence_basis.rs` pins the cap→stage
  mapping.
- Fixture goldens pin the producer-emitted rows (`cargo xtask fixtures`).

## Implementation Mapping

- `crates/ripr/src/output/stage_rows.rs` — row projection, JSON emitter,
  agent line.
- `crates/ripr/src/output/json/report.rs` — emission beside the `ripr`
  object.
- `crates/ripr/src/output/human/agent.rs` — `stages=` line after records.
- `crates/ripr-contract/src/domain/confidence_basis.rs` — `capped_stage`.
- `schemas/ripr/check.schema.json`, `docs/OUTPUT_SCHEMA.md` — contract.

## Metrics

- `unit_test_pass_rate`
- Fixture-golden additive drift count (regeneration touch count) recorded in
  the landing PR.
