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

The `EvidenceState` actionability, `GateMode` top-level status and
`FixInstructionState` repair-route selectors use the same declaration/guard
discipline in the application proof domains. Symbolic indices are bounded by
the declared slice length. Each input constructor has its own cover before
the production predicate runs, alongside the existing semantic outcome covers.
The literal predicates, warning/configuration-entry bounds and gate unwind
bound remain unchanged. The gate input helper returns immutable zero/one-entry
slices over an empty string: the producer reads only slice emptiness, so the
proof need not model allocation or vector cleanup to cover those same inputs.
Native cover bindings must name the new constructor
properties and retain their own source/configuration and claim generations;
an older outcome-only cover receipt does not establish the new proof domain.

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

The command exits nonzero when any harness is `FAILED` or `NOT-ESTABLISHED`,
when `cargo kani` is not runnable, and when no harness is found unless
`--allow-empty` is passed. `--allow-empty` is a local opt-out for sparse
checkouts; the CI lane never passes it. It cannot admit incomplete cover
evidence for a nonempty harness selection.

### Machine observation journal (#14472)

The runner also writes `target/ripr/reports/kani.json` and immutable run-owned
JSON/Markdown checkpoints. One typed observation record owns both projections.
A non-success checkpoint is published before discovery or a child process;
consumers must bind the expected run ID, not infer freshness from file presence.
Terminal publication failures fail the command. Concurrent convenience aliases
are last-published, not a guarantee of which invocation started most recently.

The journal records selected source harnesses, their observed proof-file bytes,
process arguments/status/deadlines, captured text and commitments, parsed native
block identities and existing-parser cover counts. Missing selected results,
unexpected native blocks, changed/unreadable proof files, or exceeded retained-
text budgets cannot earn lane success. The unique selected harness-file set
must equal the observed source-file set, with one observation per file. Missing,
extra, duplicate or same-count substituted observations refuse admission, even
if every observed file rechecks successfully or `--allow-empty` is present.
Multiple harnesses in one file share its one observation. This is an internal
inventory-consistency invariant, not complete native discovery or consumed-source
closure. Complete historical rows survive a
partial failure; they do not override the aggregate refusal.

This is `ripr.kani-observation.v1`, not a registered-claim receipt. Claim IDs and
generations remain null, registered claims remain zero, and claim assurance is
not established. Proof-file rechecks are not complete or atomic consumed-source
custody. Captured text is not guaranteed to preserve original pipe bytes.
Provider/solver authenticity and process-tree cleanup remain unobserved.
The existing preflight deadline repair remains separately owned by #12581.

See the [observation contract and maintenance runbook](../verification/kani-observation.md)
for byte budgets, publication/retention rules, schema, consumer requirements and
qualification commands. The existing workflow retains JSON beside Markdown;
this amendment does not add workflow subscriptions or required contexts. The
current-main event-retirement policy must survive stack integration.

The module and process controls are mapped by
`.ripr/traceability.d/14472-kani-observation.toml`. They require actual native
execution separately from schema and source checks; authored tests alone do
not establish the journal's behavior.

### The lane

`.github/workflows/kani.yml` is advisory and not a required check. It runs
nightly, on dispatch and when `merge-ready` is applied to a pull request that
touches either owning crate source, with `kani-verifier` pinned to an exact version. See
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
  `cover!` on both outcomes and every input constructor.
- Application proof-domain ordinary tests check constructor uniqueness and the
  literal actionability/repair-route predicates. Compiler-refusal controls must
  separately exercise a real enum addition, an omitted row and a duplicate row;
  a guard-removal control distinguishes compiler coverage from example tests.
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

- `xtask/src/formal_evidence/kani_output.rs`: discovery, complete-cover
  classification, native cover captures, unique attribution, and ambiguity
  controls. `lane_verdict` remains the single shared aggregate owner.
- `xtask/src/formal_evidence/native/tests.rs`: exact native property projection,
  multiline descriptions, pinned compiled inventory, invocation/profile refusal,
  and consistency between typed observations and the shared document validator.
- `xtask/src/kani_lane.rs`: process reconciliation, partial timeout, argument
  parsing, missing-tool/empty-lane behavior and complete report rendering.
- `xtask/tests/kani_lane_process.rs`:
  `kani_runner_requires_observed_complete_cover_evidence` and real child-process
  controls. Their provider output is synthetic, not a native proof.
- The #14469 constant-only witnesses are composed here: `production_cap_is_256_mib`,
  `exit_code_constants_match_documented_table`, `generation_counter_saturates_at_max`,
  and `default_budget_is_valid`. Source composition does not itself qualify the
  full live lane or grant protected native assurance.

## Implementation Mapping

- `xtask/src/formal_evidence/kani_output.rs`: shared pure interpretation and report.
- `xtask/src/formal_evidence/native_inventory.rs`: compiled inventory and exact
  native observation attribution.
- `xtask/src/formal_evidence/native/profile.rs`: pinned observed tool headers and
  aggregate process semantics; no tool installation or authenticity claim.
- `xtask/src/kani_lane.rs`: execution and process reconciliation.
- `crates/ripr/src/kani_smoke.rs`: the smoke harness.
- `crates/ripr/src/proof_domains.rs`: guarded application constructor domains.
- `.github/workflows/kani.yml` and `policy/ci-lane-whitelist.toml`: the lane.
- `Cargo.toml`: `unexpected_cfgs` check-cfg for `cfg(kani)`.

## Metrics

- `unit_test_pass_rate`: the xtask tests above are required Rust tests.
