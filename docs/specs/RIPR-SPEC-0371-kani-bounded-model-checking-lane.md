# RIPR-SPEC-0371: Kani bounded model checking lane states exactly what a proof covers and never passes by absence

Status: proposed

Owner: repository / policy

Created: 2026-10-31

Linked proposal:

- None yet

Linked ADRs:

- None yet

Linked plan:

- None yet

Linked issues:

- #12109: internal Kani claim boundaries.
- #12468: ambiguous harness-to-result joins.
- #14460: positive cover evidence before verification admission.
- #12579: bounded pre-run Kani availability discovery.

Linked PRs:

- None yet

Support-tier impact:

- None. Kani proofs are an internal engineering check on `ripr`'s own code.
  They add no user-visible claim and change no finding vocabulary or tier
  (see [support tiers](../status/SUPPORT_TIERS.md)).

Policy impact:

- Register this spec in `policy/doc-artifacts.toml` (via
  `policy/doc-artifacts.d/`) and `.ripr/traceability.d/`.
- Adds the `kani` lane to `policy/ci-lane-whitelist.toml`, the
  `.github/workflows/kani.yml` workflow and the `cargo xtask kani` command.
- Adds `unexpected_cfgs` check-cfg for `cfg(kani)` to the workspace lints.

## Problem

Some load-bearing logic in `ripr` is a small pure core (for example the
actionability flip, which must stay fail-closed) where a
bounded model check can establish a property for every input in a stated
domain, which example-based tests cannot. Without a lane that is honest about
what ran, a missing tool, a harness that never executed or missing cover
evidence could read as a pass.

## Behavior

### Harnesses

- A harness is a `#[kani::proof]` function in a `#[cfg(kani)]` module placed
  next to the code it proves. Ordinary builds never compile it.
- A harness calls the production function. It does not prove a copy.
- Input bounds are explicit `kani::assume` statements with a comment naming
  the domain they model.
- Each harness carries at least one `kani::cover!` on its interesting path,
  directly or through a called helper. Constant-only harnesses also witness
  that their checked operation is reached. An unsatisfied required cover fails
  the lane's reachability policy; it does not establish that every assertion is
  vacuous or that the implementation has a functional counterexample.
- Code Kani cannot model (heavy string or `HashMap` work, async, process or
  filesystem effects) is not forced through a harness. A smaller pure core is
  proved, or the limit is recorded; a pass is never fabricated.

### Domain decision obligations (#12109 / #11912)

The feedback, executed-control, causal-delta and probe harness input domains
use constructor lists whose wildcard-free matches are generated from the same
declaration. Adding a variant or omitting a listed constructor must fail to
compile in both ordinary tests and Kani builds; duplicate constructors are
rejected. This guard does not fabricate enum values or depend on numeric
discriminants, variant counts, or a separately maintained match.

Feedback judgment expectations are explicit per reason, preserving the five
meanings in SPEC-0179. Useful actions, limitations, correct targets and
discriminators, and clear explanations are useful. Wrong targets/discriminators,
false actionability, duplicates, already-covered items, incorrect causal
attribution and stale results are incorrect. Missing context, ambiguity and
unclear explanations are unclear. Command failures, overly broad routes,
slowness, noise and unsupported workflows are expensive. Intentional defensive
code, intentional low grip and preferred manual review mean intentional
no-action. `Other` has an unclear default and requires a note; receipt validation
still requires an explicit judgment override.

The SPEC-0183 scalar `claims_satisfaction` predicate accepts both `Passed` and
`Substituted`, and rejects each other state. Both directions are checked, with
separate reachable witnesses for both positive states and every refusal state.
This is a claim-state predicate, not evidence admission: the surrounding packet
validator still decides whether an actual pass or substitute is acceptable.
The outcome partition also checks both directions: ordinary positive tests,
review prose, structural discrimination, a command that never exercised its
subject, and `NotExecuted` are unexecuted substitutes. Failed-before/passed-after
and rejected-wrong-implementation observations are not. `InstrumentUnavailable`
is separately represented and also is not classified as an unexecuted
substitute. This distinction does not by itself establish admissibility; a
declared, artifact-backed substitute remains separately validated.

Ordinary tests call the same production predicates over the complete guarded
domains. Literal expected behavior is independent of the production predicates.
Each symbolic constructor gets a reachability witness. Native verification,
ordinary execution, and wrong-implementation/compile-refusal controls must be
reported separately, bound to their actual source and tool configuration.
Fixed identity strings and other bounds of existing harnesses remain unchanged;
this does not establish packet validation, full-program behavior, or a full-lane
qualification.

### The command

`cargo xtask kani [--allow-empty] [--harness <name>] [--default-unwind <n>]
[--timeout-secs <n>] [--harness-timeout-secs <n>]` discovers harnesses under `crates/ripr/src` and `crates/ripr-contract/src`, runs
`cargo kani` once per owning package (`ripr` and `ripr-contract`) for them,
with one shared whole-run deadline and writes `target/ripr/reports/kani.md` with one
row per harness. Each row is exactly one of:

- `VERIFIED`: Kani reported a successful verification, exactly one well-formed
  positive-total cover summary reports all covers satisfied, and no observed
  property status contradicts or leaves that evidence undetermined. The claim
  is bounded by the harness's assumptions and native verification policy.
- `FAILED`: Kani reported a native verification failure, or a required cover
  was known not to be satisfied. Inspect native properties to distinguish a
  counterexample, unwinding failure and a reachability-policy failure.
- `NOT-ESTABLISHED`: the tool is missing, the run timed out, the harness was
  not found, a single harness exceeded `--harness-timeout-secs` (default 900;
  CBMC reports a timeout, not a counterexample, so it is never `FAILED`), or the
  output held no successful verdict or complete positive cover evidence.

For a success-looking native result, missing, zero-total, malformed,
inconsistent or repeated cover summaries are `NOT-ESTABLISHED`, even if repeated
summaries agree. A summary that reports undetermined covers, or a regular
property status of `UNDETERMINED`, `ERROR` or an unrecognized word, also stays
`NOT-ESTABLISHED`. Raw unknown/error properties can be absent from the native
cover count; an N-of-N summary does not override them. A known cover shortfall
with no undetermined evidence is `FAILED`. A property identified by its final
`.cover.<id>` class cannot report `UNREACHABLE` against a complete cover summary,
or substitute `SUCCESS`, `FAILURE`, `COVERED` or `UNCOVERED` for the supported
cover statuses; these contradictions are `NOT-ESTABLISHED`. An ordinary
assertion's `UNREACHABLE` status does not by itself identify an unsatisfied cover.

The supported format is the pinned Kani 0.68.0 default regular output. Its
[cover-summary renderer](https://github.com/model-checking/kani/blob/0d2328a93f0e0ff66132d6bfa1a7d884877cf862/kani-driver/src/cbmc_property_renderer.rs)
prints `N of M cover properties satisfied` only for a positive total, with
optional nonzero undetermined and unreachable counts in that order. An
unsatisfiable cover contributes to the total without an extra suffix. Native
success can coexist with unsatisfied covers, so the lane checks reachability
separately. A native failure retains its failing disposition; a native timeout
retains its `NOT-ESTABLISHED` disposition.

A row can consume a result only when its bare name is unique in the selected
discovered inventory and exactly one parsed native block matches it. Duplicate
source names, multiple matching qualified names and repeated identical headers
are `NOT-ESTABLISHED`, independent of result order or whether the duplicates
agree. They do not borrow a verification time. An unrelated unique row keeps
its own result. This is conservative name-based attribution, not a claim that
the source path establishes the native module identity.

The native process exit is checked separately from each text verdict. A nonzero
or absent exit status or a whole-run timeout cannot produce a successful
aggregate even if every parsed row looks verified. On a partial run, separately
observed complete rows retain their results while unfinished or absent rows
remain NOT-ESTABLISHED; the report discloses the process-level problem. Native
failure does not convert a failed harness into an unobserved timeout or upgrade
a successful-looking row into complete-run assurance. This does not establish
native-source provenance or inventory completeness.

The `cargo kani --version` availability probe uses the shared bounded child
owner and requires both output streams to finish. Its separate deadline is the
smaller of 10 seconds and the requested `--timeout-secs` verification budget
(with a one-second minimum). A failed, signaled, status-less or timed-out probe
reports an explicit NOT-ESTABLISHED reason and never starts verification.
This preliminary allowance is **in addition to** the subsequent
`--timeout-secs` verification deadline. Process cleanup and bounded pipe drains
can add their own grace periods; the probe budget is not a whole-command
deadline. `--allow-empty` never overrides unavailable tooling.

Discovery retains distinct safe failure classes: executable/interpreter not
found, permission-denied launch, other launch failure, process observation or
termination failure, incomplete/failed capture, descendant-cleanup failure,
missing Kani subcommand, and unsuccessful exit. The missing-subcommand class
requires Cargo's exact uncolored stderr line `` error: no such command: `kani` ``
and exit code 101. Other nonzero exits stay ordinary unsuccessful exits.
Reports contain fixed diagnostic vocabulary and process exit status, never
arbitrary version stdout, stderr or low-level error text. Successful discovery
retains its text capture for callers; it does not authenticate native proof or
provide byte-exact artifact custody.

The command exits nonzero when any harness is `FAILED` or `NOT-ESTABLISHED`,
when `cargo kani` is not runnable, and when no harness is found unless
`--allow-empty` is passed. `--allow-empty` is a local opt-out for sparse
checkouts; the CI lane never passes it. It cannot admit incomplete cover
evidence for a nonempty harness selection.

### The lane

`.github/workflows/kani.yml` is advisory and not a required check. It runs
nightly and on explicit dispatch, with `kani-verifier` pinned to an exact
version. The ordinary PR/`merge-ready` subscription is retired under #4842;
proof still covers both owning packages, `ripr` and `ripr-contract`. See
[CI](../CI.md#advisory-kani-lane).

## What a proof does and does not claim

A `VERIFIED` row records native verification success within the stated
`kani::assume` bounds and unwinding depth, with a positive satisfied cover
inventory. Ordinary harnesses check enabled assertion and safety properties;
expected-panic harnesses use Kani's expected-panic success policy. A required
cover witness establishes its reachability, not the correctness or completeness
of the chosen functional specification.

It does not claim:

- anything about inputs outside the harness's assumptions or beyond the unwind
  bound;
- anything about code the harness does not call, or about the full program;
- runtime mutation outcomes, test sufficiency or static exposure results. Real
  mutation testing remains a later independent authority;
- that the harness states the right property. A wrong assertion proves the
  wrong thing; harnesses are reviewed like any test oracle.

## Required Evidence

- The xtask unit tests pin the status classification: an explicit success
  verdict and complete positive cover evidence are required for `VERIFIED`.
  Known unsatisfied covers fail the reachability policy; missing, malformed,
  repeated, zero-total, inconsistent and undetermined evidence stays
  `NOT-ESTABLISHED`. Ordinary unreachable assertions and expected-panic success
  remain compatible with complete cover evidence.
- The real runner/process controls check exit status and persisted report for
  positive and incomplete cover evidence, with both `--allow-empty` settings.
  Their child output is synthetic and does not establish native verification.
- Native compatibility evidence is bound to the actual source and pinned tool
  separately from parser/process tests. Missing-tool and zero-harness controls
  remain failing lane results.
- The smoke harness `evidence_state_actionable_only_for_actionable` proves the
  actionability predicate holds for exactly one `EvidenceState` variant, with
  `cover!` on both outcomes.
- Oracle discrimination: injecting a bug into `EvidenceState::is_actionable` makes the
  smoke harness fail (recorded in the introducing PR).

## Known Limits

- Kani compiles each owning library; a run takes minutes and several GB of
  memory.
- Harness discovery is textual (`#[kani::proof` followed by `fn name`) and
  rejects duplicate names in the selected inventory. It does not establish
  a complete native harness inventory, source-to-module identity or source/run
  provenance.
- The lane trusts Kani's own output verdict lines; a change to their wording
  degrades to `NOT-ESTABLISHED`, never to a pass.

## Non-Goals

- A full mutation engine, proof system or coverage claim.
- Verifying string- or filesystem-heavy analysis code.
- Making the lane a required check.

## Acceptance Examples

1. With the tool absent, `cargo xtask kani` reports `NOT-ESTABLISHED` and fails.
2. With a harness whose `cover!` is unreachable, the row is `FAILED`.
3. With a clean harness set, every row is `VERIFIED` and the command succeeds.
4. Two same-named declarations cannot consume one successful block; multiple
   matching or repeated blocks also remain `NOT-ESTABLISHED`. `--allow-empty`
   does not bypass this nonempty ambiguous result.
5. Bare native success, zero declared covers or undetermined cover evidence
   produces `NOT-ESTABLISHED`; the command fails even with `--allow-empty`.
6. Complete satisfied covers can accompany an unreachable ordinary assertion
   or an expected-panic result without changing the native policy.

## Test Mapping

- `xtask/src/kani_lane.rs`:
  `discovery_finds_named_harnesses_past_attributes_and_docs`,
  `classification_requires_an_explicit_success_verdict`,
  `classification_requires_positive_complete_cover_evidence`,
  `undetermined_properties_never_become_verified`,
  `complete_cover_evidence_allows_unreachable_ordinary_assertions`,
  `complete_cover_evidence_preserves_expected_panic_success`,
  `cover_property_and_summary_must_not_contradict`,
  `pinned_native_cover_captures_keep_distinct_lane_dispositions`,
  `unsatisfied_cover_fails_the_reachability_policy`,
  `blocks_join_to_harnesses_and_a_missing_block_is_not_established`,
  `duplicate_source_names_never_share_a_native_result`,
  `duplicate_native_blocks_are_not_first_match_wins`,
  `native_process_error_cannot_be_hidden_by_verified_rows`,
  `partial_timeout_preserves_completed_rows_but_never_passes`,
  `mixed_failure_and_absent_results_keep_distinct_dispositions`,
  `unique_native_join_retains_status_time_and_separator_boundary`,
  `lane_never_passes_by_absence`,
  `arguments_parse_and_reject_unknown_flags`,
  `report_names_every_status_word`.
- `xtask/tests/kani_lane_process.rs`:
  `kani_runner_requires_observed_complete_cover_evidence`.
- Constant-only harness witnesses:
  `production_cap_is_256_mib`, `exit_code_constants_match_documented_table`,
  `generation_counter_saturates_at_max`, `default_budget_is_valid`.

The Linux synthetic-process integration controls in
`xtask/tests/kani_version_probe.rs` exercise normal and unavailable discovery,
the failure classes above that can be induced at the real command boundary,
both owning packages, persisted reports and the empty opt-out. An independent
watchdog reexecutes the test in an owned session, uses file-backed captures and
retains the primary's wait identity until all live session members are stopped.
Its clock does not call the probe's deadline implementation. A controlled hung
probe with an inherited-pipe descendant proves this watchdog fires before the
probe deadline and confirms both descendant identities are no longer live.
The existing isolated-session controls continue to cover normal completion,
cleanup failures and unrelated-session preservation. These controls do not run
Kani and make no non-Linux process-tree claim.

For focused maintenance, `cargo test -p xtask --test formal_runner` compiles the
actual `kani_lane.rs` and `run.rs` modules with their existing tests and the
same `automation` dependencies as the CLI. Shared ANSI normalization and the
test CWD gate are included from their production/test owners. This target
does not compile the unrelated `main.rs` test suite. The command controls run
separately through `cargo test -p xtask --test kani_version_probe` against the
actual CLI binary.

On Linux, a host that cannot code-generate the full analyzer facade can execute
the same runner source and lane API with a smaller dependency graph:

```sh
cargo test -p xtask --no-default-features --features formal-runner --test formal_runner
```

This mode includes the original `crates/ripr/src/process_owner.rs` source with
`cfg(test)` and reexports the actual `ripr-contract` cancellation module, just
as the facade does. It uses the same timeout, cleanup and capture code, with
the owner's existing test-only observation/failure-injection fields enabled.
It runs the availability fixtures under `probe_controls::`, calling the real
`kani_lane::run_lane` in the isolated child. API errors use libtest's exit code
101 and its wrapped stdout; full CLI errors use exit code 1 and exact report
stdout. These are distinct proof boundaries. Narrow-mode success does not
establish the full xtask dispatcher, facade artifact, non-Linux behavior or
native Kani proof. Automation/default production imports remain unchanged.

Useful focused selectors include
`run::tests::complete_capture_rejects_a_pipe_that_outlives_the_drain_grace`,
`run::tests::typed_capture_retains_cleanup_failure_when_both_drains_also_fail`
and the `run::test_session::tests::` group. Native Kani execution and hosted
platform coverage remain separate evidence.

## Implementation Mapping

- `xtask/src/kani_lane.rs`: discovery, classification, report and verdict.
- `crates/ripr/src/kani_smoke.rs`: the smoke harness.
- `.github/workflows/kani.yml` and `policy/ci-lane-whitelist.toml`: the lane.
- `Cargo.toml`: `unexpected_cfgs` check-cfg for `cfg(kani)`.

## Metrics

- `unit_test_pass_rate`: the xtask tests above are required Rust tests.

### Native custody and compiled inventory (#14472 / #12470)

The lane additionally emits `target/ripr/reports/kani.json`, using the shared
formal-evidence DTO and admission predicates. Markdown projects that same DTO.
Unregistered native observations cannot become registered semantic assurance.
The execution record binds exact package/source/qualified harness identities,
source/configuration before/after observations, each process, bounded native
streams and retained SHA-256 commitments. Missing, extra, duplicate, stale or
unattributable native subjects prevent complete success. Exact compiled source
identity may distinguish same-named harnesses in different files/modules; a
bare-name-only join still follows the existing conservative refusal law.

Native listing uses the pinned compiled Kani inventory in a fresh run-owned
working directory. The producer cannot accept a pre-existing `kani-list.json`.
Kani configuration is admitted independently of its hash: custom checking flags,
unsupported compiler configuration and unregistered replacement/stub routes
cannot preserve the default-profile interpretation. Source identity and
native success do not establish production-body correspondence or proposition
adequacy; those retain their registered-claim owners.

`cargo xtask kani --replay <report>` checks retained byte commitments and reruns
the same supported interpretation without invoking the verifier. It is a
historical replay, not a current-source or authenticated producer claim.
[The result contract](../formal-verification/RESULTS.md#native-capture-implementation-14472--12470)
names the supported profile, resource limits, retention and nonclaims.
