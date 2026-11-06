# RIPR-SPEC-0422: Structured staged-evidence rows

Status: proposed

Owner: product-output

Created: 2026-11-05

Linked issues:

- [EffortlessMetrics/ripr-swarm#12415](https://github.com/EffortlessMetrics/ripr-swarm/issues/12415) (round-11 finding: staged evidence a script must parse English)
- [EffortlessMetrics/ripr-swarm#13162](https://github.com/EffortlessMetrics/ripr-swarm/issues/13162) (agent-efficiency study, Nov 2-4: delivered-context cost of prose-only evidence)

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
  capped, which related tests back it — had to parse sentences, because no
  row form tied the three together.
- `ripr check --format agent` and `ripr explain --format agent` carried no
  stage states at all: a gap record named the class and the strongest test,
  but an agent reading only the compact surface could not see that reach was
  `unknown` or that discrimination was `no` without opening the 11 KB JSON
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
- `tests` — optional, present on the `discriminate` row only: the
  contributing related-test references (`file`, `line`, `name`, and `miss`
  when the producer recorded one) the stage was judged over, bounded to the
  same window the document renders under `related_tests`, so every reference
  resolves inside the document. Assertion text is never duplicated here; it
  stays in `related_tests[].oracle` and `assertion_texts`. The other stages'
  producers do not attribute specific rows, so their rows carry no `tests`
  field; absence is the explicit "no per-stage attribution" signal, and a
  consumer must not infer attribution from it.

`schema_version` stays `0.2`: the change is additive per the check contract's
bump rule ("nested `finding` … may grow additively within the pinned
version"), the same rule #10700 and #7258 used. `json-terse` codes nothing in
the new field: `basis` and `tests` entries ride the document unchanged
(states and tokens are short shared vocabulary), and expanding the terse
legend reconstructs the same rows.

### `stages=` compact row on the agent surfaces

`ripr check --format agent` prints one `stages=` line directly after each
finding record (gap, `open=`, and limited records):
`stages=reach:yes,infect:weak,propagate:unknown,observe:yes,discriminate:no`.
`ripr explain --format agent` prints the same line after its one record.
States use the same closed vocabulary as the JSON rows; the line names states
only (basis and test references stay JSON-side), and runs with no records
emit no `stages=` lines.

## Required Evidence

- `crates/ripr/src/output/stage_rows.rs` is the single shared projection
  (rows, JSON emitter, agent line) with unit tests pinning: the five-row
  order and vocabulary; that stage-named basis entries land on their own
  stage row and finding-level caps land on none; that only contributing rows
  inside the JSON window are referenced, with `miss` carried; that rows other
  than `discriminate` carry no `tests`; and the compact agent line.
- `crates/ripr-contract/src/domain/confidence_basis.rs` pins the
  `capped_stage` mapping exhaustively over the closed cap set.
- `fixtures/boundary_gap/expected/check.json` (and every fixture golden
  regenerated in the same additive pass) pins the emitted rows for a real
  producer finding, including the `discriminate` row's test references.
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
- No per-stage test attribution for `reach`/`infect`/`propagate`/`observe`:
  no producer records it, and a renderer must not re-derive classifier
  decisions (establishes-reach depends on owner-kind facts the finding does
  not carry).
- No schema_version bump, no removal or rename of existing fields.
- No new stage states, cap codes, or exposure semantics.
- Context-packet, evidence-record, repo-exposure and SARIF surfaces keep
  their existing shapes.

## Acceptance Examples

- A `weakly_exposed` Rust finding (the `boundary_gap` fixture) renders:
  `"evidence_stages": [{"stage":"reach","state":"yes","confidence":"medium"}, …,
  {"stage":"discriminate","state":"yes","confidence":"medium","tests":[
  {"file":"tests/pricing.rs","line":4,"name":"below_threshold_has_no_discount"},
  {"file":"tests/pricing.rs","line":9,"name":"far_above_threshold_discounts"}]}]`
  and `--format agent` prints
  `stages=reach:yes,infect:weak,propagate:yes,observe:yes,discriminate:yes`
  after the gap record.
- A TypeScript preview finding capped by `observation_not_confirmed` renders
  that cap's code and summary on the `observe` row only; the
  `boundary_input_not_matched` cap appears on no row because it names no
  single stage.
- A finding whose related-test window was capped renders `tests` references
  only for rows present under `related_tests` in the same document.

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
  object; window constant shared with `stage_rows`.
- `crates/ripr/src/output/human/agent.rs` — `stages=` line after records.
- `crates/ripr-contract/src/domain/confidence_basis.rs` — `capped_stage`.
- `schemas/ripr/check.schema.json`, `docs/OUTPUT_SCHEMA.md` — contract.

## Metrics

- `unit_test_pass_rate`
- Fixture-golden additive drift count (regeneration touch count) recorded in
  the landing PR.
