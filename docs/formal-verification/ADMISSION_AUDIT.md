# Formal evidence document admission audit

Status: proposed internal developer tool. Qualification belongs to the exact PR/run evidence, not this page.
Owner: #14472 (machine-result contract), #14473 (claim/result relationship),
under #12109 and #12904. Architecture and maintenance documentation remain in
[PR #14471](https://github.com/EffortlessMetrics/ripr-swarm/pull/14471).

## Purpose and authority

`formal-evidence-audit` is a product-independent, offline Rust helper in the
existing unpublished `xtask` package. It checks supplied draft `FormalClaimV1`
and `NativeProofRunV1` documents for contradictions. It is not a second verifier,
registry, result issuer, native-output parser or CI finalizer.

The document types, decoder, admission predicates and their tests have one owner in
`xtask/src/formal_evidence.rs`. The binary only handles arguments, rendering and
exit status. Native capture and registry candidates extend this shared module;
they must not keep a second private interpretation of these contracts.

The format names and fields follow #14471. The helper adds semantic checks that
JSON Schema alone cannot express, such as matching a result's claim generation,
subject and required-cover count against a supplied claim. Its internal schema
is not a released product API.

**Exit 0 means consistent input documents, not a successful proof.** Every
successful JSON response contains `native_assurance: "not_assessed"`. In
particular, an honestly reported counterexample, timeout, stale result or tool
absence can be a consistent document. Never use this exit code as the Kani lane
result or as a required proof check.

## Shared native interpretation prerequisite

`xtask/src/formal_evidence/kani_output.rs` owns source discovery, text and native
property interpretation, result attribution, the row-based lane verdict and
escaped Markdown report rendering. The existing lane imports that same owner.
It retains command execution and process-completion policy. Complete positive
cover admission and its native captures from #14469 now live in the shared
parser; bare success, contradictory or incomplete covers remain insufficient.

`native_inventory.rs` decodes pinned Kani 0.68.0 list-format 0.1, compares exact
compiled/source subjects and attributes observations to unique qualified native
blocks. Properties and cover projections come from the same block; duplicates
retain an explicit refusal with no borrowed property set. Native failure,
unsupported expected panic, missing data, unwind failure and timeout remain
distinct. Multiline descriptions preserve their native bytes and cannot hide
metadata continuations. The profile helper checks the supplied Kani/CBMC headers
and aggregates observed process outcomes without turning stdout into process
success. These are pure interpretation APIs, not fresh verifier executions.

Retained fixtures include native inventory/provenance and a native multiline
counterexample. Their historical source identities and byte hashes remain
unchanged. The library's direct controls also use explicitly synthetic source
bindings and process observations; fixture replay never becomes a current proof.

This stage depends on #14551/#14549 and #14469. The latter still owns its four
production reachability witnesses and full-lane integration. #14545 retains
producer activation. Cargo configuration/toolchain capture, source-closure
discovery, raw artifact replay and registry binding remain separate adoption
stages. No new producer, registry, workflow or authority is installed here.
The document-audit binary keeps `native_assurance: "not_assessed"` and exit zero
still means document consistency. Changes to the pinned native format require
new retained captures and the direct refusal controls below before activation.

## Commands

Build and exercise the isolated target with the repository-pinned toolchain:

```bash
cargo test --locked -p xtask --no-default-features \
  --features formal-evidence-audit --lib
cargo clippy --locked -p xtask --no-default-features \
  --features formal-evidence-audit --lib --bin formal-evidence-audit -- -D warnings
```

Validate one or more claim documents, optionally joined to a run:

```bash
cargo run --locked -p xtask --no-default-features \
  --features formal-evidence-audit --bin formal-evidence-audit -- \
  --claim path/to/claim.json --run path/to/run.json
```

Repeat `--claim` for each document. Omitting `--run` validates declarations only.
Each claim file contains one claim object, not an array. The run file is a single
run object. `--help` describes exit codes; malformed input exits 2. Output is
JSON on stdout; diagnostics are on stderr. The helper writes no files, invokes
no subprocess, installs no tools and performs no network access.

The feature reuses the existing Serde, serde_json and SHA-256 dependencies. Default
`automation` includes it, so ordinary workspace target qualification does not
silently omit its tests. The isolated commands avoid the optional `ripr`
dependency; they are not substitutes for the repository's integration gates.
Input reads are bounded to 1 MiB per file, 16 MiB total, and 256 claim files.
These are byte/admission bounds, not a wall-time or exact peak-memory guarantee.

## Checks implemented

| Boundary | Refusal or retained distinction |
| --- | --- |
| Decoder | Unknown fields, unknown closed enums, duplicate keys (including bound maps), missing required nullable keys, trailing data and unsupported schema names reject. |
| Claims | Empty/duplicate IDs, zero generations, blank proposition/domain/observable, missing nonclaims/trust/owners/closure/subjects/negative controls/currentness, empty bounds, invalid unwind and path traversal reject. |
| Critical claims | Every provider requires nonempty reachability obligations. The provider-specific descriptions declare intent; they are not observed reachability. |
| Identity | Results must reference selected registered claims and their exact declared subjects. Duplicate result subjects reject. A generation mismatch must remain explicitly stale. |
| Established row | Requires declared verified/current evidence, discovery membership, complete cover counts matching the claim, duration, raw stream/block commitments and the exact registered assumption digest. Success reasons may be empty. |
| Failed row | Requires a declared current counterexample or unreachable required cover, discovery membership, the claim's required-cover count, duration, raw stream/block commitments, the exact assumption digest and a nonempty status explanation. An unreachable-cover verdict must report a positive unreachable count. Tool failure, absence and verified output cannot be relabeled as behavioral failure. |
| Partial run | Completed rows remain inspectable after whole-run failure or timeout. Missing selected subjects and unmapped discovered subjects are reported separately. Neither is erased by a successful row. |
| Complete inventory | Empty, missing, unmapped or rejection-bearing inventories cannot declare completeness. Every selected subject must occur in native discovery; a missing-result placeholder cannot fill that gap. `undiscovered_selected_subjects` reports this separately from missing result rows. |
| Process | Completed needs an exit observation; failed/killed cannot claim exit zero; timeout fields must agree; not-started cannot borrow output or a native verdict. |
| Zero observations | A not-started or partial run can have no rows. Not-started rows may retain declared required-cover counts, but cannot claim satisfied/unreachable covers, duration or peak memory. No dummy claim/result is invented. An incomplete inventory needs a reason. |
| Presentation | Output ordering is deterministic; document consistency, declared positive rows, process success, cleanup and native assurance remain different fields. |

Only the Kani bounded-model-check native profile is admitted. Other native
profiles are explicitly unsupported, not silently converted to Kani semantics.
Other claim families can still be inspected as declarations without a native
run. Provider versions are declarations, not a qualified parser compatibility
matrix.

`admitted_merge_group` is accepted claim-phase metadata for the #14500 target
lifecycle. Legacy ordinary-PR and merge-ready phase spellings remain inspectable
historical declarations, not permission to start proof workflows. This passive
audit selects no claims and grants no CI admission for any phase.

## What this cannot establish

The helper does not inspect the current repository or verify the supplied
source closure. It does not read raw output artifacts, recompute their digests,
authenticate a producer, prove source-to-binary provenance, decide whether a
`current` flag is true, or compare actual target/features/configuration/bounds.
Artifact/source digest syntax is checked; truthful bytes and their custody are
not. The assumption digest is additionally recomputed from the supplied claim's
compact UTF-8 JSON array, preserving order, as defined in [RESULTS.md](RESULTS.md).
This establishes consistency between declarations, not that the provider used
those assumptions. Even an empty array has a non-null digest on an observed row.

Cover-count equality does not establish cover identity, assertion dominance or
non-vacuity of every assertion. Required negative controls are declarations,
not executed experiments. A valid identifier does not prove the proposition
is correct or that its owner exists. JSON object-key ordering is irrelevant;
assumption-array order remains part of its exact commitment. Changing
a proposition while retaining its generation is a separate base/head authority
check that this helper deliberately does not pretend to perform.

The run's `claim_registry_generation` is retained as a declaration. No canonical
registry-digest algorithm is invented here. The production join must compare it
against the actual governed registry/source capture before assurance is issued.

## Design-contract corrections carried by this candidate

The inspected design at `51aeb532` required at least one result row. Real tool
absence, zero selection and interruption before an observation need a zero-row
failure document, not fabricated evidence. The revised schema allows this case,
while complete inventory still requires nonempty subjects and results.

The revised schema also rejects local status/currentness/native-verdict
contradictions, missing raw commitments on positive rows, impossible process
states, empty critical claim metadata and placeholder digest strings. Cross-row
identity, count equality and selected-subject completeness are checked in Rust;
JSON Schema validation alone still does not establish those relations.

The examples now reference the actual
`crates/ripr-contract/src/domain/evidence_state.rs` owner and
`crates/ripr/src/kani_smoke.rs` harness. The sample run explicitly says
`not_started`, with no native results. Its zero IDs/digests are illustrative
placeholders, not authenticated source commitments. The smoke harness manually
selects five constructors; the example does not claim compile-time exhaustiveness
or invent an unwind attribute absent from the source.

`examples/admission-controls.json` retains twelve synthetic contradictory
observations and one honest zero-observation control. Nine contradictions are
rejected by the revised JSON Schema; the other three require relational/count
checks. The Rust corpus test exercises all thirteen through the same auditor.
No fixture is a native execution receipt.

## Integration and maintenance

#14472 should use one production semantic validator at the runner/report boundary
and retain genuine raw captures plus process observations. This helper is an
executable contract prototype, not yet that installed producer. Move or reuse
its validated predicates there; do not maintain two divergent admission rules.
#14473 should consume those admitted observations beside current governed
claims, with missing/stale/unsupported evidence visible. No registry is
introduced by this slice.

Before adoption, run the isolated native tests and Clippy above, formatting,
`cargo xtask check-traceability`, `cargo xtask check-doc-index`,
`cargo xtask check-process-policy`, `cargo xtask ci-policy`, and the selected
`check-fast` route. Preserve any unexecuted or failed dimension. Qualification
of this helper does not qualify #14469's native parser, #12581's process deadline,
the full Kani denominator, or product-facing Kani/Verus journeys.

For every contract change, retain a closest valid control and a one-field
counterexample. The authored tests cover missing evidence, wrong identity,
duplicate/unknown input, missing nullable fields, partial timeout, zero rows,
stale generations, unmapped subjects and deterministic ordering. These tests
must be compiled and executed before the candidate is called qualified.

Adding a formal provider requires its own semantics and native captures; do not
extend a string allowlist and call that support. Claim weakening, replacement,
retirement and source movement still require #6452/#6456's base/head law.
Required-check changes remain separate from this local helper.

Rollback removes this additive helper, feature and documentation through the
ordinary protected route. It neither deletes retained native evidence nor
changes the existing Kani runner, workflow, branch protection or public output.

## Native execution declarations (#14472)

The shared `Run` type and draft run schema now also represent captured native
execution, separate unregistered harness observations, exact byte-artifact
commitments, source snapshots, effective configuration, installed tools,
compiled inventory, process observations and registered-result absence. This
is the common document contract for the native producer and registry follow-ons.

The auditor checks consistency of these declarations. A successful lane
declaration requires complete source/configuration observations, a complete
selected inventory, successful bounded invocation records, completed cleanup
and positive cover evidence. The declared result status must agree with its
native verdict, full selected source identity and cover/property projection.
A partial or not-started run can retain missing
source identities without inventing commitments. Required nullable keys must
still be present with explicit `null`; omission is rejected by both the shared
decoder and schema. Observed registered results
still require those commitments. Unregistered observations do not increase
registered claim coverage.

The existing CLI and its guarantee remain unchanged: exit zero means supplied
documents are consistent, and the result keeps
`native_assurance: "not_assessed"`. Neither claimed currentness nor a supplied
VERIFIED row authenticates a verifier execution. This slice installs no native
producer, raw-stream replay, claim registry, source-closure scanner, workflow
subscriber or required check. Those follow-ons reuse this module instead of
creating another admission authority.

Focused maintenance continues to use the library test selector and actual
`formal-evidence-audit` entrypoint documented above. The native document
controls exercise false currentness, incomplete cover identities, missing
metadata, omitted observations, abnormal aggregate exit, missing preflight,
cleanup not observed, and duplicate or escaping manifest paths. Their fixture
is synthetic and is never native proof evidence. The strict schema continues
to reject unknown fields, while semantic predicates enforce relationships
that JSON Schema cannot compare across records.

Rollback removes this additive execution-declaration extension and its schema
fields together. Preserve the parent admission laws and all historical proof
artifacts. Any future wire-law change must update the shared DTO, schema,
semantic controls, producers and consumers in one reviewed composition.
