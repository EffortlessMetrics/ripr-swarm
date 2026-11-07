# Muse plugin evaluation input

`muse-plugin-eval-v1` is the canonical record coordinated with the
[Muse plugin evaluation suite in PR #13109](https://github.com/EffortlessMetrics/ripr-swarm/pull/13109).
The package is a separately delivered change, not a runtime dependency of
this external-record consumer. The existing DX scoreboard consumes that same record. Its optional
`observations` block carries structured, evidence-bound measurements; it is
not a second record format or a nested schema version.

This is an ingestion foundation, not an agent runner, plugin qualification,
canonical repair validator or effectiveness benchmark. The consumer checks
shape and internal consistency. It does not execute commands, fetch evidence
URLs, authenticate external artifacts or prove that observations occurred.

## Contract

Use the existing route:

```bash
cargo xtask dx-scoreboard --boards agent --ingest path/to/muse-trial.json
```

Exactly one Muse record is admitted per invocation. A repeated document/run
and a second distinct Muse record both refuse: retain separate reports. Other native
input sources may coexist. External generic `ripr-dx-scoreboard-input-v1`
records claiming `source: muse-plugin` refuse at the file-loading boundary;
only the validated native adapter may project those generic rows internally.
This is not a second CLI or a change to other sources' ingestion rules.

The closed [schema](../../schemas/ripr/muse-plugin-eval.schema.json) retains
the suite's required `identity`, `environment`, `task`, `mcp` and `verdicts`
fields, plus its optional model, repair, execution, efficiency and provenance
context. Existing null/omitted optional context remains valid.

Without `observations`, ingestion retains the complete canonical record and
emits all 16 axes as `not_measured`. A prose `completed_repair`, `ready` setup,
reported command exit zero or numeric efficiency count does not become a
measurement. No run, trial, model or artifact identity is invented.

When supplied, `observations` requires:

- `run_id`, `trial_id` and `recorded_unix_ms`; no nested `schema_version`.
- `subject`: exact plugin package SHA256 and version, RIPR full source commit
  and version, Muse version, platform, model identifier and model-configuration
  digest. Record the configuration separately; a model name does not identify
  its sampling, reasoning or context configuration.
- `scenario`: scenario ID, task ID and instruction digest, fixture ID and
  fixture-content digest. Hash the actual inputs, not their prose description.
- `evidence`: unique artifact IDs, typed kinds, retained URIs/paths and
  SHA256 digests. Before/after references identify separate observations.
  Evidence URIs are opaque; ingestion does not read them or validate their
  claimed digests. The native input file itself gets its actual byte digest
  and path recorded by the loader.

At least one dimension must be present inside `observations`. Optional
structured dimensions and measurements are omitted, not null or guessed zero.
Whitespace-only structured identities, unknown properties, malformed pins and
unsupported static classes refuse. Structured counts must be exact and
between zero and 2^53 - 1; elapsed milliseconds
may be fractional but must be finite and nonnegative. Raw JSON numbers are
schema-validated before the generic numeric projection can round them.
Mathematically integral decimal/exponent spellings such as `100.0` and `1e2`
are accepted for integer fields; source fractions are still rejected before
rounding. Execution exit codes use the separate signed 32-bit range.

### Canonical context and structured evidence must agree

Known outer scenario, fixture, RIPR/plugin/Muse version, binary commit,
platform, model and trial identities must match their structured counterparts.
`subject.platform` repeats `environment.os`; runner class remains separate
context. `identity.fixture_digest` may be the bare SHA256 or `sha256:<digest>`.
Null, omitted, `unknown` and `not_measured` identity context does not overwrite
structured pins. The workspace `environment.git_head` is **not** the tested
RIPR binary's `identity.ripr_commit`; they may differ.

`scenario.task_sha256` must hash the exact UTF-8 bytes of
`task.initial_request`. When `model.configuration` is supplied, its exact
UTF-8 bytes must hash to `subject.model_config_sha256`, including an empty
configuration string. No configuration is inferred when that field is absent.
The fixture and external artifact digests remain supplied pins, not verified
contents.

Supplied `repair.canonical_id`, attempt ID and before/after classes are joined
to corresponding structured fields when both are known. A seam ID is not a
canonical gap ID; MCP snapshot freshness is not durable-attempt currentness.
Raw execution command strings may contain retries or shell syntax and are not
parsed into argv or joined by the first matching role. Only structured
execution observations drive those metrics.

The importer rejects `completed_repair` when supplied structured outcomes
refute it: failed setup, a known-negative static after class (`weakly_exposed`,
`reachable_unrevealed` or `no_static_path`), a non-ready durable after state,
or a completed execution leg inconsistent with pass/assertion-fail/pass.
`infection_unknown`, `propagation_unknown` and `static_unknown` do not refute
completion; they leave the static metric `not_measured`, never zero.
Absent/incomplete observations stay unknown even if the outer record asserts
completion. The outer judgment never creates a metric or certifies a repair.

## Independent observations

The input names below are relative to `observations`.

| Input | Projected metric suffix after `agent.muse_` | Claim ceiling |
| --- | --- | --- |
| `setup` | `setup_passed` | Supplied plugin-load and MCP-availability observations agree. |
| `static` | `static_exposed_after` | The selected gap's supplied after class is `exposed`; this is static evidence only. |
| `durable` | `durable_ready_reported` | An identified durable attempt is reported current, compliant and `ready_to_finish`, with retained before/after manifest references. |
| `executions.original` | `original_test_passed` | The selected test reportedly passed on the intended implementation. |
| `executions.counterexample` | `wrong_control_test_failed` | That test reportedly failed by assertion on the deliberately wrong implementation. |
| `executions.restored` | `restored_test_passed` | That test reportedly passed after restoring the intended implementation. |
| All three execution legs | `executed_discrimination` | The complete supplied pass/assertion-fail/pass sequence supports the named control, not general adequacy. |

There is no overall repair-success metric. Setup does not imply static
exposure, durable repair or executed discrimination. A prose transcript cannot
substitute for evidence typed as a repair manifest. A durable observation needs
an actual `repair-attempt-<24 lowercase hex>` identifier and separate retained
before/after manifest references; readiness without `current: true` and a
compliant edit cage refuses. These supplied fields do not replace RIPR's
canonical manifest and workspace validation. Ordinary descendant HEAD movement
is not prohibited by this importer. A ready after state requires changed
manifest bytes; a failed/non-ready attempt may leave those bytes unchanged
while retaining distinct before/after observation references.

`static` binds one gap and keeps the before/after classes. A before-only record
does not imply an after observation. Unchanged classes may legitimately have
identical report bytes; changed classes cannot claim the same artifact digest.
The actual supplied class is retained even when it is explicitly unknown.
An unknown static result may coexist with complete executed discrimination;
neither the executed metric nor the outer judgment fills in missing static
knowledge or creates an overall repair-success metric.

`executions` names one test selector and one exact command argument vector
shared by its three independently identified observations. Each leg binds the
implementation bytes, test bytes and retained execution evidence. All legs
must use the same tests; the restored implementation must equal the original,
and the counterexample must differ. A passing leg needs exit zero and a
nonzero executed-test count with no failure. An assertion failure needs a
nonzero exit, executed tests and `failed_test` equal to the named selector.
A compiler, timeout, infrastructure or unrelated-test failure is not a
discriminating assertion. The importer does not infer argv semantics or check
that a runner actually executed the command it reports.

Each completed outcome maps to 1 or 0 for that single observation. Missing or
incomplete observations map to explicit `not_measured` samples, never success
or numeric zero. In particular, a missing/incomplete restored leg cannot
produce successful `executed_discrimination`. An incomplete-only record still
retains its run and evidence identities in the report.

## Resource observations, not efficiency conclusions

`observations.usage` accepts optional `model_calls`, `tool_calls`, `elapsed_ms`,
`peak_rss_bytes` and a token block. It needs a usage-report evidence reference
and `complete`, which describes accounting completeness, not repair success.
Partial observed totals retain their numbers as incomplete samples, so smaller
partial counts cannot meet a target.

Tokens keep `input` (including cached input), `cached_input`, `output` and
`total` separate. The importer requires cached input ≤ input and
total = input + output, then derives uncached input by subtraction. The five
token metrics are `input_tokens`, `cached_input_tokens`,
`uncached_input_tokens`, `output_tokens` and `total_tokens`. Missing token
accounting produces no token number. Numeric outer call/token counts must
agree where the same structured measurement is supplied. Numeric
`efficiency.wall_time` is in seconds and must equal `elapsed_ms / 1000`;
strings such as `not_measured`, `unknown` or `"1500"` remain opaque context.
Other raw efficiency fields are preserved without metric promotion. No dollar
metric is emitted: the canonical `reported_cost` field does not identify
whether it means API list-price estimates or actual subscription spending.

Resource metric declarations reuse the scoreboard's context-only convention:
`review_on_change = true` with a zero higher-is-better floor. This is **not**
a recommendation to consume more or an empirical performance target.
Outcome targets of 1 express logical one-trial criteria, not measured success
rates. No measured baseline is added.

## Report identity and interpretation

For structured observations, the sample key is
`muse:<run_id>:<canonical-record-sha256>`. For a sparse record it is
`muse-document:<canonical-record-sha256>`: document identity, **not** a trial
identity. Any changed
subject, fixture, observation or accounting changes that identity; a reused
run ID cannot join different evidence through the generic baseline comparator.
An identical replay has identical data, not a new independent observation.
All Muse baseline comparisons explicitly report `comparable: false`, including
an identical replay and incomplete accounting. This source-scoped rule leaves
other scoreboard comparisons unchanged. This version does not provide
population aggregation or efficacy/trend testing.

The supplied record is retained once, in the first native sample's detail.
Other native samples point to that same record and metric. The file loader adds
the original input path and byte SHA256 once. Preserve the original receipt
and referenced artifacts outside this report as well.

The report header's revision and runner describe the **ingestion environment**.
The tested RIPR/plugin/Muse build, model and platform are the supplied subject
pins. A green DX regression gate is not plugin certification: existing
`--gate` checks regressions and instrument/comparison failures, not every
below-target outcome. Read the independent sample statuses and missing axes.

## Test evidence and limits

[The committed example](../../tests/fixtures/verification/ripr/muse-plugin-eval.valid.json)
is a synthetic setup-only **contract test fixture**, not a measured run.
Its zero timestamp, test-only versions, synthetic artifact digests and
`test-fixture://` references are intentional fixture data. Only the task and
model-configuration digests hash actual strings in that fixture. It is not a
baseline or a reconstruction of the historical cold-install prose receipt.

`xtask/src/reports/dx_scoreboard/muse_plugin/tests.rs` covers native file
loading and generic ownership, schema/pin/numeric negatives, missing and
incomplete dimensions, contradictory success, typed artifact joins,
pass/assertion-fail/pass separation, token arithmetic, one-run admission,
generic-source bypass, no-movement reports and changed baseline identities.
Sparse canonical records, null/omitted context, contradictory envelopes,
separate identity namespaces and seconds/milliseconds conversion are also
covered. Neither historical Markdown receipt is backfilled into fabricated
machine-readable observations.

The producer sweep registers this consumer-only input as exempt from producer
counting, not exempt from validation. Its hand-written fixture is an edge
fixture. `native_muse_schema_sweep_keeps_synthetic_input_as_edge_only` in
`xtask/src/schema_producer_sweep.rs` pins zero producer subjects and one
validated edge fixture; the existing inventory test requires a binding for
every published schema. No test fixture is promoted to measured producer output.

```bash
cargo test -p xtask --bin xtask native_muse
cargo xtask check-verification-contracts --check
```

Those tests establish importer behavior when executed. A native Muse run,
durable repair artifacts, targeted wrong-implementation controls and empirical
performance remain separate evidence, required before claiming those outcomes.
