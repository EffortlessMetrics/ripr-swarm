# Critical claim registry and assurance scoreboard

`.ripr/formal-claims.json` is the initial versioned registry for #14473. The
`formal-assurance` binary validates it, reads explicit source snapshots and
optional retained native results, and renders JSON or Markdown from one DTO.
It does not launch a verifier, Cargo, Git, a shell, a network request, or a CI
workflow. Building the binary is a separate caller action.

The [native capture and maintenance runbook](NATIVE_CUSTODY.md) owns pinned
installation, exact registered invocations, replay, retained artifacts and the
positive/mutant/restored collection procedure.

The shared `formal_evidence::Claim`, `Run`, document auditor and native replay
remain the contract owners. Registry metadata adds semantic-family grouping,
exact source/authority rules, correspondence declarations and concrete
negative mutations. The existing `fv-scoreboard` source census remains
separate: unregistered harnesses never increase registered assurance.

## Initial obligations

| Claim | Family | Native subject or remaining owner |
| --- | --- | --- |
| `ripr.classification.exposed-gating` | Classification/actionability | `exposed_iff_all_gating_stages_yes`; source extraction remains outside scope |
| `ripr.gate.command-error-exit` | Gate/exit/aggregation | `command_error_exit_code_total_and_matches_table`; dispatch and aggregate rows remain outside scope |
| `ripr.currentness.candidate-action` | Currentness/identity/causality | `currentness_action_and_base_side_are_disjoint`; producer currentness is a separate obligation |
| `ripr.resources.bounded-input` | Resource/budget safety | `read_cap_and_exceeds_limit_are_exact`; the I/O/allocator premise remains trusted |
| `ripr.edit-cage.path-boundary` | Edit/repair/disposition | `rule_match_respects_path_boundaries`; five-path corpus, not arbitrary filesystem containment |
| `ripr.repair.readiness-producer-conformance` | Edit/repair/disposition | Manual SPEC-0192/#12109 producer, cage, readiness and final-disposition review |

Each row contains real production and proof locators, explicit bounds,
nonclaims, owner links, cover anchors and a unique before/after mutation with
its expected failed claim. These retained declarations do not claim execution.
Their default scoreboard state is `retained_no_admitted_execution`. Only an
explicitly supplied, admitted positive/mutant/restored pair changes that state.
A separately retained native mutant run keeps its actual execution result;
the missing-admission field does not claim it never ran. Compile failure,
timeout and an unrelated assertion cannot be counted as discrimination.

## Build and use

```sh
cargo build --locked -p xtask --no-default-features \
  --features formal-assurance --bin formal-assurance

target/debug/formal-assurance --root . --format json
target/debug/formal-assurance --root . --format markdown
target/debug/formal-assurance --root . --format claim-index
```

Use the actual target directory if `CARGO_TARGET_DIR` is set. The helper writes
only stdout. Redirect generated JSON and Markdown to explicit run-owned paths;
`--check-output FILE` compares the requested format with an existing output and
refuses drift. Both formats use the same normalized scoreboard. The
`claim-index` format emits #14542's exact projection, including its registry
generation; it implements no affected-selection or workflow policy.

To inspect one retained native run, add `--run FILE --artifacts DIRECTORY`.
There is no automatic search for a mutable latest result. The native owner
replays the retained raw streams, inventory, covers and configuration, verifies
artifact hashes, then the registry checks the exact registered source and
claim binding. A row is `ESTABLISHED` or `FAILED` only under that local evidence
law. Provenance remains `local_retained_bytes_checked_not_authenticated`:
byte custody does not authenticate a producer, grant CI authority, or establish
specification adequacy. Supplying a success-shaped JSON document alone cannot
produce an established row.

Required covers also need an explicit native mapping for the pinned provider:
the exact description, repository-relative source file, function, line and
column emitted by Kani. The passive matcher requires one distinct observed
property per semantic cover and refuses missing, extra, duplicated or borrowed
properties even if aggregate counts match. Absolute locations are normalized
only by the native owner against the captured repository root. A missing
mapping stays visible as an unqualified mapping; its claim cannot become
established through a count-only join. A source move or provider-format change
requires reviewing these mappings with fresh retained native output.

The initial exact mappings are qualified for the six currentness covers under
Kani 0.68.0. Other initial Kani claims retain their semantic cover declarations
but have no qualified native presentation mapping yet. Their missing mappings
remain an explicit gap. The currentness format sample alone is not a registered
run or evidence that the integrated native collector executed successfully.
Registry generation 2 and currentness claim generation 2 additionally register
the measured negative control's exact first assertion, including its multiline
description. The retained `currentness-mutant.stdout` format fixture and its
receipt under `xtask/tests/fixtures/kani-native-output/` record one failed
assertion with all six covers satisfied. That earlier sample remains format
evidence; only a new trilogy on the frozen current generation earns paired
admission.

Without a native run, the five Kani rows remain `NOT_ESTABLISHED` and the manual
row remains `NOT_PROVEN`. An applicable unsupported provider stays
`UNSUPPORTED`; incompatible source/generation/profile evidence is `STALE` or
refused. Missing claims, required-cover evidence and executed negative controls
remain visible. The report has no aggregate formal-coverage percentage.

`--changed-owner PATH` may be repeated to expose supplied critical owner paths
without a current applicable claim. These inputs are not authenticated change
facts, and an empty input is not a complete change classification.

## Paired native negative controls

`--control-pair FILE` adds a passive comparison to the supplied positive run:

```sh
target/debug/formal-assurance --root . \
  --run /path/to/evidence/positive/kani.json \
  --artifacts /path/to/evidence/positive \
  --control-pair /path/to/evidence/control-pair.json --format json
```

The closed `ripr.native-control-pair.v1` manifest has these fields. Its references
are local retained bytes; a manifest declaration is not an execution receipt.

| Field | Required value |
| --- | --- |
| `schema` | `ripr.native-control-pair.v1` |
| `claim_id`, `claim_generation`, `registry_generation`, `control_id` | Exact current prepared registration and declared mutation |
| `positive`, `mutant`, `restored` | Each contains `run: {path, sha256, bytes}` and `artifacts: "relative-directory"` |
| `original_source`, `mutant_source` | `{path, sha256, bytes}` for the complete original and mutated production file |

All artifact paths and directories are normal relative paths beneath the
manifest's parent. Symlinks and parent traversal are refused. Retain the three
complete native artifact directories, the two production source images and
the pair manifest together. Up to sixteen explicit pairs can be projected;
duplicate control identities are refused. JSON and Markdown expose the same
admitted observations and the remaining gaps.

The consumer reuses the shared native replay for every run. Both positives must
be distinct, current, registered `ESTABLISHED` observations with the exact
source, generation, assumptions, covers and bounds. The supplied main run must
be the pair's original positive. The mutant stays unregistered. Its native
result must be a completed assertion counterexample with clean process custody,
the same reachable domain and required covers, and exactly the registered
failed assertion properties. Each `expected_failures` entry declares the
property identity and exact native description/source location. Empty mappings
remain an explicit admission gap; aggregate failure counts are insufficient.
In the mutant only, a later ordinary assertion outside that required failure
set, in the same harness and with the ordinary assertion description, may be
`UNREACHABLE` because Kani prunes the intended failed assertion's
continuation. That allowance does not satisfy an intended failure or admit an
unreachable required cover or safety check. All required domain and semantic
covers must still be satisfied.

Retained source images must reproduce one occurrence of the declared `before`
to `after` replacement in a reviewed production file separate from the
registered proof-authority files. Whole captured source manifests must differ at exactly
that production file, with no added or omitted helper, harness, domain or
configuration member. The restored source manifest must equal the original.
Provider/compiler/solver commitments, effective native arguments, declared
resource policy and selection must agree across all three records. Remaining
invocation time can vary with preparation time; shared replay checks each
observed timeout against its declared policy. Configuration
captures include source hashes, so their digests legitimately differ for the
mutant: comparison removes **only** the governed production-file key and
requires every other input and Cargo configuration key to remain identical.

An admitted control is labeled `local_native_discrimination_observed`, with
`local_retained_bytes_checked_not_authenticated` provenance. It establishes
only the recorded discrimination of this exact mutation under these bounds.
It does not authenticate who executed the runs or their timing, measure a
mutation score, complete the scheduled mutant-lane owner #12920, or establish
specification completeness. A partial set of controls remains visibly partial.

To maintain a control, first qualify any changed native assertion presentation
against retained output, review the declared mutation and increment the
affected generations. Freeze the resulting registry and source before
collecting the admitted positive, single mutant and restored positive. Do not
reuse a format sample from a different source generation as the final pair.

## Source and generation maintenance

The Kani source rule conservatively includes the whole selected package's
`src` tree, the dependent `ripr-contract` tree where applicable, workspace and
package Cargo inputs, optional package build script, `.cargo`, `xtask/src`, and
`tools/repo-policy`, plus the shared `process_owner.rs` and contract cancellation
module compiled by the native collector. It includes registered owners and proof-authority
files. The shared passive Rust scanner also follows literal `include!`,
`include_str!`, `include_bytes!` and module `path` inputs, with the same
unsupported-syntax and test-only distinctions as native capture. Preparation
records these paths; a native join requires exact equality with the independently
captured input set for the claim's package. This conservatively includes some
embedded assets that are irrelevant to the particular scalar proposition.
The same rule projects a captured native manifest: deleting a helper
from a supplied binding and recomputing its digest cannot omit that helper from
the independent captured closure. This broad rule makes no minimal-impact or
CI-cost claim. Dependency/compiler semantics remain named trust assumptions.
Preparation also checks the actual registered function's Kani attributes:
one ordinary proof, its exact declared unwind override, and no replacement or
panic-inverting attribute. The admitted default unwind must be explicitly
present in both the registration and native execution profile.

The registry generation is a SHA-256 commitment to normalized registry data,
including its positive authored generation. Claim/list ordering is canonical;
the exact assumptions array order is preserved. Assumptions use the shared
compact UTF-8 JSON digest, including `[]`. Source manifests use the shared
native canonical-manifest serializer. These formats are distinct contracts.

When the proposition, domain, assumptions, covers, bounds, provider, trust,
source rule, result law or proof-authority bytes change, increment the claim
generation and registry generation. A production-only change invalidates its
native source result even if the proposition remains the same. Recheck source
anchors and the retained mutation; do not invent a new passing result.

Compare against an explicit predecessor with:

```sh
target/debug/formal-assurance --root . \
  --previous-root /path/to/predecessor-source \
  --previous-registry .ripr/formal-claims.json
```

Without both predecessor inputs the transition is **`NOT_ASSESSED`**.
Same-generation metadata weakening or changed proof-authority bytes is refused,
including an input-domain restriction whose assertion text stays unchanged.
Exact file relocation may preserve a claim generation only through a named
replacement record and equal actual predecessor/successor authority bytes.
Function/body changes need a new generation. Deleting an entry is not accepted
retirement. A generation increase is `REVIEW_REQUIRED`, not approval of reduced
protection; #6452/#6456 retain the broader base/head authority decision.

## Validation and limits

```sh
cargo test --locked -p xtask --no-default-features --features formal-assurance --lib
cargo clippy --locked -p xtask --no-default-features \
  --features formal-assurance --bin formal-assurance -- -D warnings
```

Controls cover unknown/duplicate vocabulary, missing critical fields, exact
source anchors, source omissions, metadata and harness weakening, relocation,
stale result axes, unauthenticated success documents, manual/unsupported
obligations, normalized ordering and machine/human parity. Paired controls
exercise extra source edits, closure omissions, changed tool configuration,
wrong assertion identities, unrelated failures and indeterminate outcomes.
Actual native
qualification belongs to the producer route; unit fixtures do not substitute
for it. Registry validation checks local spec paths and positive issue IDs;
maintainer review verifies the linked GitHub ownership without adding network
execution to the passive command.

Inputs are bounded: 4 MiB document reads, 8 MiB per source file, 256 MiB unique
source bytes, 20,000 source files and the shared 256-claim bound. Source reads
refuse symlinks and traversal. Before/after equality is not an immutable
filesystem snapshot or producer authentication.

Rollback removes this registry/projection through an ordinary revert while
preserving the source census and native-result history. It does not retire the
underlying obligations. No new CI subscriber or required-check promotion is
part of this slice.
