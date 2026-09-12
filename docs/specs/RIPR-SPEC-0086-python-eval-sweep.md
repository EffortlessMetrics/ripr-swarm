# RIPR-SPEC-0086: Python Tier A External-Repo Eval Sweep

Status: accepted

Acceptance note (2026-08-14): #1161 landed the Tier A `cargo xtask
eval-sweep` command (`xtask/src/command.rs` dispatch plus
`xtask/src/reports/eval_sweep.rs`). The run algorithm's classification
contract was completed by #3259, which threads the captured exit status
into `classify` — a nonzero exit after parseable JSON reads `crash`, as
step 3 requires, pinned by classifier and `run_check` boundary tests on
both hosts. The Required Evidence golden exists as
`rendered_report_matches_golden_from_fixed_run_vector`: byte-exact JSON
and Markdown from a fixed two-run vector (stable `ok`, unstable
`parse_failure`), so every rendering change must re-bless it
deliberately. Tier B judgment semantics live in RIPR-SPEC-0092, which
remains proposed.

Owner: language-adapter / swarm

Linked proposal:

- None. This is a standalone evidence-tooling contract; it adds no product
  library behavior and no public API. It anchors the eval-sweep-driven Python
  reliability campaign tracked by
  [#1160](https://github.com/EffortlessMetrics/ripr-swarm/issues/1160) and
  `plans/python-repair-routing/` (the former `.ripr/goals/` tracker was
  deleted with the goals scheduler, #2056).

Linked ADRs:

- [ADR 0009](../adr/0009-python-parser-substrate.md) (Python parser substrate;
  the sweep measures the current `rustpython-parser`-backed lane).

Linked plan:

- [RIPR-PLAN-0017: Python Repair Routing Implementation Plan](../../plans/python-repair-routing/implementation-plan.md)

Linked issues:

- [release(py): Python usable-tier readiness checklist](https://github.com/EffortlessMetrics/ripr-swarm/issues/1160)
- [eval(py): validate the retained eight-repository sweep manifest and run schema](https://github.com/EffortlessMetrics/ripr-swarm/issues/3565)

Linked PRs:

- [#1161](https://github.com/EffortlessMetrics/ripr-swarm/pull/1161) — Tier A
  command and report implementation.
- [#3259](https://github.com/EffortlessMetrics/ripr-swarm/pull/3259) — captured
  exit-status classification and boundary regressions.
- [#3261](https://github.com/EffortlessMetrics/ripr-swarm/pull/3261) — fixed-run
  JSON/Markdown golden test and accepted lifecycle reconciliation.

## Problem

The in-repo Python dogfood corpus is saturated: every metric passes on a curated
set authored by the same people who wrote the analyzer. That confirms the repair
loop is internally consistent, but it says nothing about how `ripr check` behaves
on Python repositories we did not write. The release-readiness question —
**does the analyzer stay crash-free, parse-robust, and gap-ID-stable on external
code?** — has no measured answer.

This spec defines a **Tier A external-repo eval sweep**: a report-only `xtask`
command that runs `ripr check` over a pinned manifest of real external Python
repositories and records only machine-checkable robustness facts. Tier A is a
stability floor; it deliberately does **not** judge actionability or usefulness
(that is Tier B, a later spec).

## Behavior

### One production delta

Add `cargo xtask eval-sweep`: a report-only command. It introduces no change to
`crates/ripr` (the analyzer library) — it is automation that exercises the
existing `ripr check` surface and aggregates results.

### Inputs

- A pinned manifest (`fixtures/python-eval-sweep/manifest.json`): a versioned
  envelope listing external repos as `{ id, url (https), sha, license, shape,
  synthetic_diff?, why }`.
- A synthetic Python diff per repo (or a shared fallback diff). The diff is read
  from a file; the external repo working tree is never mutated.

### Run algorithm (per manifest entry)

1. Resolve the repo checkout. When `--clone` is passed, clone at the pinned
   `sha` into a `target/`-local checkout dir via the existing
   `run::run_with_envs` helper; otherwise expect a pre-placed checkout and record
   `skipped_missing_checkout` if absent (never fails on absence).
2. Run `ripr check --root <checkout> --diff <synthetic-diff> --mode fast --json`
   under a wall-clock timeout via `run::capture_output_with_timeout`. The harness
   builds `ripr` once and invokes the built binary directly (not `cargo run`), so
   `runtime_ms` measures analysis time rather than cargo's per-invocation overhead.
3. Classify the outcome from exit code and JSON:
   - `crash` — process abort or `ripr: <err>` failure exit;
   - `timed_out` — exceeded the timeout;
   - `parse_failure` — JSON parsed but the file degraded to a named static-unknown
     limitation (graceful, **not** a crash);
   - `ok` — exit 0 with well-formed JSON.
4. Collect the set of `canonical_gap_id` values and the run-1 runtime. From the
   same run-1 JSON, also tally — per repo and aggregate — the `classification`
   distribution (the 7 exposure classes), and, where the Python sink-alignment
   fields are emitted (RIPR-SPEC-0028), the `oracle_alignment` distribution and
   repair-packet presence counts. No extra invocation: these are read from the
   already-captured JSON.
5. Re-run steps 2–4 once; gap-ID stability is `set(run1) == set(run2)`. The
   distributions are taken from run-1 only; the re-run is for gap-ID stability.

### Metrics

Across non-skipped entries: `crash_rate`, `parse_failure_rate`, `timed_out_count`,
runtime min/median/max/total, and `gap_id_stability_rate`. The run also records,
across the `counts_as_run` set, the `classification_counts` and `alignment_counts`
distributions. **Distributions are descriptive only: they are informational, make
no usefulness/actionability judgement, and never affect `gate_status`.** The
`gate_status` is:

- `not_run` when `repos_run == 0` (zero repos analyzed — e.g. a default no-clone
  run with no pre-placed checkouts). A pass/review verdict is only meaningful once
  at least one repo was analyzed, so an empty run is **never** a vacuous `pass`.
- `pass` when `repos_run >= 1`, `crash_rate == 0`, and `gap_id_stability_rate == 1.0`.
- `review` when `repos_run >= 1` and the crash/stability gate failed.

Each carries a recorded reason. Empty `repos_run` guards division (rates default
to `0.0` crash / `1.0` stability).

### Accepted-manifest and retained-receipt validation (`eval-sweep check`)

`cargo xtask eval-sweep check [--manifest <path>] [--runs <receipt>]` is the
typed semantic validator for the accepted artifacts (offline: no repository
materialization, no RIPR execution, no lookups beyond the two artifact files):

- One loader owns manifest and run-row semantics. The strict
  duplicate-key-rejecting parse is the check-path contract; the lenient
  in-memory parse in the sweep-report path stays historical-tolerant on
  purpose (check validates retained accepted artifacts, the report path
  renders in-flight sweeps), and the two converge when #3566/#3567 own the
  refresh/report commands. The accepted manifest
  (`python_eval_sweep_manifest`) must declare schema/kind/spec/tier and
  **exactly eight** uniquely identified subjects — the canonical denominator.
  Subject content is data-driven: any eight well-formed subjects validate, not
  only the retained fixture bytes. The manifest schema is closed (deny-unknown):
  the owned top-level keys are `schema_version`/`kind`/`spec`/`tier`/
  `description`/`limits`/`synthetic_diff`/`repos` and the owned per-subject
  keys are `id`/`url`/`sha`/`license`/`shape`/`synthetic_diff`/`why` plus the
  optional `tree_digest`/`snapshot`/`provenance`/`retention_class` identities —
  exactly the keys the canonical fixture carries, so unknown keys are schema
  rot, not forward compatibility. License is required (null/empty fails); an
  optional identity is either absent (typed `incomplete`) or well-formed — an
  explicit null, an empty string, or a malformed value fails, because a
  present-but-garbage identity is not an absent one.
- Retained run receipts (`python_eval_sweep_report`) validate in two owned
  shapes. Schema `0.2` is the historical shape the sweep command writes.
  Schema `0.3` adds the currentness identities: binary/features/config/profile/
  input identity, materialization/detection/corpus-selection/execution states,
  the `complete`/`partial`/`parse-failed`/`timed-out`/`crashed`/`unsupported`/
  `tempfail`/`stale` status vocabulary, raw/output/evidence digests,
  repeat-run comparison identity, and a sha256 manifest-digest binding.
- Fail closed (nonzero exit, diagnostic names subject/field/reason plus the
  deterministic rerun command) on: duplicate or missing subjects, a changed
  denominator, receipt rows contradicting the manifest pins, a receipt
  identity that contradicts the manifest binding (when the manifest and the
  receipt both record a comparable identity — `license`, `tree_digest`,
  `snapshot`, `provenance`, `retention_class` — they must match; a receipt
  value with no manifest side to bind discloses `incomplete` on the manifest
  side instead of fabricating a binding), unsafe
  (non-portable, absolute, or secret-bearing) paths and URLs, unknown state
  vocabulary, contradictory status (e.g. `complete`/`partial` with an
  execution state that did not run — `partial` with `not-executed` included —
  or stable gap IDs listed as unstable in either direction, or a false
  stability claim whose `unstable_gap_ids` list is omitted; each at the 0.2
  row level or inside the 0.3 `repeat` block), duplicate identity copies
  inside one receipt that disagree (the row-level `tree_digest`/`snapshot` vs
  the `repository` block, and a row's `binary` identity vs the receipt-level
  `ripr` block — a mismatch fails naming both locations), a known field whose
  recorded value has the wrong type or shape for its emitted shape
  (`description`/`gate_reason`/`why` are strings, `stderr_excerpt` is a
  possibly-empty string, `gap_ids`/`limits` are arrays of strings), malformed digests,
  a stale manifest digest (receipt bound to different manifest bytes), a
  required summary aggregate missing from an analyzed receipt, a nonzero
  analysis-bearing aggregate on a zero-run receipt, and
  hand-edited aggregates that disagree with the derived rows (denominator,
  outcome counts, stability counts, runtime aggregates, rates, distributions,
  gate status). Aggregate agreement is checked in both directions: an analyzed
  (run-status) row must carry the aggregate source evidence the sweep records
  on every row (`runtime_ms`, both distributions, and the 0.2 `gap_ids_stable`;
  0.3 stability lives in the optional `repeat` block, so a recorded summary
  stability value over rows lacking it fails while an unrecorded one is
  disclosed `incomplete`), an analyzed receipt must carry the full emitted
  summary (a deleted aggregate would silently disable its row-agreement check;
  the stability aggregates are required exactly when the rows fully evidence
  stability), summary distributions must equal the row-derived
  key set exactly (zero-valued buckets included; with zero run rows the
  zero-run summary law bounds every recorded bucket to zero and the recorded
  keys are still vocabulary-checked), the supplied `gate_status` must
  equal the gate derived from the rows (`not_run` at zero runs, `pass` only
  with zero crashes and full per-row stability evidence, `review` otherwise),
  and runtime totals and distribution merges use checked arithmetic (overflow
  is a structured failure naming the aggregate field, never a panic).
- Failed, unavailable, timeout, parse-failed, unsupported, partial, and stale
  rows remain selected: they are valid rows and stay in the denominator. A row
  that did not run must carry no analysis counts, and `repos_run == 0` gates to
  `not_run` — never a vacuous `pass`, and a `pass` claim requires per-row
  stability evidence and zero crashes. The not-a-vacuous-pass law extends to
  the summary: with zero run rows every analysis-bearing aggregate
  (classification/alignment counts, runtime min/median/max/total, stability
  counts and rate) must be zero or absent — any nonzero value is a fabricated
  claim about rows that never ran.
- Missing identities are typed `incomplete` with a per-field disclosure; they
  are not invented and not errors (the retained manifest carries no
  provenance/retention/snapshot identities by design, and historical 0.2 rows
  carry no currentness fields). Missing owned identity fields — `ripr.*` at
  receipt level, and every owned field of a present row-level `binary` block
  (`digest`/`version`/`features`/`build_profile`) — disclose `incomplete` the
  same way; present-but-malformed values fail. Validation never upgrades or
  rewrites a historical receipt. `alignment_counts` is required on every
  analyzed (run-status) row, and a recorded `alignment_counts` object must
  always carry both keys — `absent` (field not emitted) and `unknown`
  (emitted value), zero-filled when empty — so the two never merge.
- Exit contract: exit 0 when every present artifact is structurally valid —
  including when identities are disclosed `incomplete` or no receipt is
  supplied (`not_run`); nonzero on any fail-closed violation. Top-level
  verdict precedence spans both artifacts: `not_run` only when no receipt is
  supplied; with a receipt, `incomplete` whenever the manifest or the receipt
  discloses incomplete identities (a complete receipt never hides manifest
  gaps), and `valid` only when both artifacts are structurally valid and carry
  zero incompletes. The verdict
  vocabulary is `valid` / `incomplete` / `not_run`: a structural
  currentness-readiness verdict, never a robustness or adequacy claim. The
  check writes `eval-sweep-check.{json,md}` only.

### Policy boundary (load-bearing)

- `--clone` is **opt-in and off the default CI path.** No `.github/workflows`
  step clones or fetches. The default command runs against pre-placed checkouts
  only, so the gated `check-pr` path never touches the network.
- All subprocess work routes through the already-allowlisted `xtask/src/run.rs`
  helpers, so no `process_allowlist.txt` or `network_allowlist.txt` change is
  required.

## Required Evidence

- This spec, registered in `policy/doc-artifacts.toml` and `docs/specs/README.md`.
- A `[[behavior]]` entry in `.ripr/traceability.toml` mapping this spec to the
  unit tests and the manifest fixture.
- `fixtures/python-eval-sweep/{SPEC.md, manifest.json, synthetic-diff.diff}`.
- Unit tests in `eval_sweep.rs`: manifest load/validate (rejects duplicate ids,
  non-https url, empty repos); outcome classifier over sample JSON; gap-ID set
  comparison flags an injected instability; metrics arithmetic with empty-set
  guards; deterministic JSON/markdown report rendering.
- A golden of the rendered report from a fixed in-memory run vector.
- The `eval-sweep check` validator tests (`python_eval_sweep` module): an
  alternate valid eight-subject manifest in a temp directory proves the
  validator is data-driven; each fail-closed shape (duplicate/missing/unknown
  subjects, changed denominator, non-https or credential-bearing URLs,
  absolute and secret-bearing paths, unknown shape tags, malformed SHAs,
  null/empty/malformed optional manifest identities, unknown
  outcome/status/state values, contradictory status (including `partial` with
  `not-executed`, and a false stability claim with its `unstable_gap_ids`
  list omitted), disagreeing duplicate identity copies (row-level
  tree/snapshot vs the `repository` block, and a row `binary` field vs its
  receipt-level `ripr` copy), wrong-typed owned fields
  (`gate_reason`/`gap_ids`/`stderr_excerpt`/`description`/`limits`/`why`),
  stale and
  malformed digests, a receipt identity contradicting the manifest binding,
  a required summary aggregate missing from an analyzed receipt, a nonzero
  analysis-bearing aggregate on a zero-run receipt, hand-edited aggregates,
  vacuous pass, pass without
  stability evidence, merged absent/unknown distributions) fails with a
  subject/field/reason diagnostic and the rerun command; missing identities
  (including `ripr.features`/`binary.features`, and a present `binary` block
  without `build_profile`) are typed incomplete, and a
  receipt value with no manifest side to bind discloses the manifest gap
  instead of fabricating a binding; a zero-run receipt gates `not_run`; the
  historical 0.2 receipt validates with disclosed incompletes and is never
  rewritten.

## Non-Goals

- No actionability, usefulness, false-actionable, or false-`exposed`
  (over-credit) judgement (that is Tier B). A robustness sweep counts emitted
  findings, so it is structurally blind to false-`exposed`: a silent over-credit
  emits nothing. Measuring it needs ground-truthed should-stay-quiet cases, not
  only boundary-flip diffs that exercise the should-gap direction.
- The `classification_counts` and `alignment_counts` distributions count emitted
  facts only. They make no usefulness, actionability, false-actionable, or
  false-`exposed` judgement, and never change `gate_status` — they add
  visibility, not a verdict. `absent` (the alignment field was not emitted) is
  kept distinct from the `unknown` enum value so the distribution is not
  silently overcounted.
- No mutation execution, provider calls, generated tests, or production-code edits.
- No network access on the default CI path; external clone is opt-in only.
- No change to `crates/ripr` analyzer behavior or public API.
- No support-tier claim; this produces evidence, not a promotion.

## Acceptance Examples

### A passing sweep

```text
repos_run = 12, crash_rate = 0.0, parse_failure_rate = 0.08,
gap_id_stability_rate = 1.0  ->  gate_status = "pass"
```

### A sweep that fails closed to review

```text
repos_run = 12, crash_rate = 0.08 (1 repo aborted)  ->  gate_status = "review",
reason = "1/12 repos crashed; investigate before promotion"
```

### Missing checkout without --clone

```text
repo skipped, outcome = "skipped_missing_checkout"  ->  excluded from rates,
never fails the command
```

### A default no-clone run analyzes nothing

```text
repos_total = 3, repos_run = 0 (all skipped_missing_checkout)
  ->  gate_status = "not_run"  (never a vacuous "pass")
```

### Accepted validation without a retained receipt

```text
cargo xtask eval-sweep check
  ->  manifest valid (8 subjects), receipt dimension "not_run",
      missing identities disclosed as incomplete  ->  exit 0 (not a pass)
```

### A hand-edited aggregate fails closed

```text
receipt rows derive repos_run = 8 but the summary claims 7
  ->  exit nonzero, diagnostic: subject=<receipt> field=`summary.repos_run`:
      hand-edited aggregate ... (rerun: cargo xtask eval-sweep check)
```

## Test Mapping

- `eval_sweep::manifest_load_rejects_invalid` -> manifest validation contract.
- `eval_sweep::classifier_maps_outcomes` -> outcome classification contract.
- `eval_sweep::gap_id_instability_detected` -> gap-ID stability contract.
- `eval_sweep::metrics_guard_empty_run_set` -> metrics arithmetic contract.
- `eval_sweep::report_render_is_deterministic` -> deterministic report rendering.
- `eval_sweep::count_distributions_tallies_classification` -> classification distribution.
- `eval_sweep::count_distributions_uses_classification_key_not_class` -> reads the real `classification` key.
- `eval_sweep::count_distributions_oracle_alignment_buckets` -> alignment distribution (`absent` ≠ `unknown`).
- `eval_sweep::count_distributions_counts_packet_completeness_presence` -> packet-presence counts.
- `eval_sweep::report_includes_distribution_and_gate_is_unaffected` -> distributions render and never change the gate.
- `eval_sweep::distribution_does_not_rescue_not_run_gate` -> `not_run` preserved.
- `eval_sweep::run_check_classifies_failure_exit_with_valid_json_as_crash` ->
  captured failure-exit boundary through the real run path.
- `eval_sweep::rendered_report_matches_golden_from_fixed_run_vector` ->
  byte-exact JSON/Markdown rendering from the fixed two-run vector.
- `eval_sweep_check::python_eval_sweep::accepts_alternate_valid_manifest_not_fixture_bytes`
  -> data-driven eight-subject acceptance (#3565).
- `eval_sweep_check::python_eval_sweep::check_artifacts_passes_on_alternate_manifest_in_temp_dir`
  -> end-to-end offline check on an alternate temp-dir manifest.
- `eval_sweep_check::python_eval_sweep::rejects_manifest_with_seven_subjects_changed_denominator`
  -> exactly-eight canonical denominator.
- `eval_sweep_check::python_eval_sweep::receipt_rejects_unknown_subject_and_missing_subject`
  -> subject-coverage fail-closed family (with the duplicate-row sibling).
- `eval_sweep_check::python_eval_sweep::receipt_rejects_hand_edited_aggregates`
  -> row/aggregate denominator agreement.
- `eval_sweep_check::python_eval_sweep::receipt_rejects_vacuous_pass_and_accepts_not_run`
  -> `repos_run == 0` is `not_run`, never a vacuous pass.
- `eval_sweep_check::python_eval_sweep::current_receipt_all_eight_statuses_validate_and_stay_selected`
  -> the full status vocabulary remains selected (denominator-preserving).
- `eval_sweep_check::python_eval_sweep::current_receipt_rejects_contradictory_status_pairs`
  -> contradictory status fail-closed family.
- `eval_sweep_check::python_eval_sweep::receipt_rejects_stale_manifest_digest`
  -> stale digest binding fail-closed.
- `eval_sweep_check::python_eval_sweep::receipt_keeps_absent_distinct_from_unknown_distributions`
  -> `absent` remains distinct from emitted `unknown`.
- `eval_sweep_check::python_eval_sweep::historical_receipt_validates_incomplete_without_rewrite`
  -> historical receipts stay historical; missing identities type incomplete.
- `eval_sweep_check::python_eval_sweep::top_level_verdict_is_incomplete_when_manifest_gaps_survive_a_complete_receipt`
  -> top-level verdict precedence across both artifacts (`valid` only when both
  carry zero incompletes; `not_run` only without a receipt).
- `eval_sweep_check::python_eval_sweep::gate_status_must_equal_the_derived_gate`
  -> supplied `gate_status` must equal the derived gate (`not_run`/`pass`/`review`).
- `eval_sweep_check::python_eval_sweep::summary_distribution_extra_bucket_fails`
  -> exact summary-distribution key-set equality (zero-valued buckets included).
- `eval_sweep_check::python_eval_sweep::analyzed_row_missing_runtime_fails`
  (with `analyzed_row_missing_distribution_fails_and_rejects_summary_totals` and
  `under_evidenced_stability_rejects_recorded_summary_and_discloses_absent`) ->
  missing aggregate source evidence never silently disables a summary comparison.
- `eval_sweep_check::python_eval_sweep::runtime_total_overflow_fails_structurally_without_panic`
  (with `distribution_merge_overflow_fails_structurally_without_panic`) ->
  checked aggregate arithmetic.
- `eval_sweep_check::python_eval_sweep::manifest_rejects_unknown_top_level_and_repo_keys`
  (with `canonical_fixture_manifest_passes_with_owned_keys_only`) -> closed
  accepted-manifest schema.
- `eval_sweep_check::python_eval_sweep::receipt_rejects_absolute_and_secret_bearing_paths`
  (with `rejects_non_https_and_credential_urls` and
  `current_receipt_rejects_malformed_digests_and_paths`) -> portable-path,
  no-secret, and digest-format fail-closed family.

## Implementation Mapping

| Concern | Code |
| --- | --- |
| Command logic (arg parse, manifest load, run orchestration, classify, metrics, render) | `xtask/src/reports/eval_sweep.rs` |
| Accepted-manifest and retained-receipt validator (`eval-sweep check`) | `xtask/src/reports/eval_sweep_check.rs` |
| Subcommand registration | `xtask/src/command.rs`, `xtask/src/dispatch.rs`, `xtask/src/reports/mod.rs` |
| Subprocess helpers (build, clone, `ripr check`) | `xtask/src/run.rs` (`run`, `run_with_envs`, `capture_output_with_timeout`) |
| Pinned manifest + synthetic diff | `fixtures/python-eval-sweep/manifest.json`, `fixtures/python-eval-sweep/synthetic-diff.diff` |
| Rendered report | `target/ripr/reports/eval-sweep.{json,md}` |
| Check verdict report | `target/ripr/reports/eval-sweep-check.{json,md}` |

## Metrics

| Metric | Meaning |
| --- | --- |
| `repos_run` | external repos analyzed (excludes skipped/clone-failed) |
| `crash_rate` | fraction of `repos_run` that aborted or failed-exit |
| `parse_failure_rate` | fraction degrading to a named static-unknown limitation |
| `timed_out_count` | repos exceeding the wall-clock timeout |
| `runtime_ms_median` | median run-1 `ripr check` wall-clock per repo (built binary, excludes cargo overhead) |
| `gap_id_stability_rate` | fraction with identical canonical gap-ID sets across a re-run |
| `classification_counts` | per-repo + aggregate 7-way exposure-class distribution (descriptive; never gates) |
| `alignment_counts` | per-repo + aggregate `oracle_alignment` distribution (`direct`/`alias`/`changed_sink_token`/`orthogonal`/`unknown`/`absent`, Python-only) plus repair-packet presence counts (`repair_placement`/`verify_command`/`python_repair_card`) |
| `gate_status` | `not_run` if `repos_run == 0`; else `pass` iff `crash_rate == 0` and `gap_id_stability_rate == 1.0`; else `review` (distributions never affect this) |
