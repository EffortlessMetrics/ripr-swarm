# Affected critical-claim planning

Status: proposed internal developer tool. The executable and schemas are a
passive contract slice under #14474; they are not an installed CI route.

Owners:

- #14474 — affected critical-claim selection;
- #14473 — governed claim registry and assurance scoreboard;
- #14472 — machine native-result contract;
- #14500 — admitted merge-group lifecycle and queue-only integration;
- #6452/#6456/#5224 — base/head verification-authority transition.

## Purpose

`formal-claim-plan` converts a supplied claim-index snapshot and supplied
change facts into a deterministic affected-critical-claim plan. It exists so
selection behavior can be reviewed and falsified before it is connected to a
native provider or protected workflow.

The helper is deliberately passive. It:

- reads one bounded JSON input;
- validates closed vocabulary, identities, repository-relative paths, claim
  generations, stack membership, and registry-generation binding;
- selects directly affected critical claims;
- widens to every critical claim when impact is unknown or incomplete;
- retains manual and unsupported obligations visibly;
- emits JSON or Markdown from one typed plan;
- writes no files and invokes no process, network, Git, provider, or workflow.

**Exit 0 means that a passive plan was produced. It grants no execution or
merge authority and establishes no native assurance.** Every JSON plan fixes
those fields to:

```json
{
  "execution_authority": "not_granted",
  "native_assurance": "not_assessed"
}
```

## Command

Build and exercise the isolated target with the repository-pinned toolchain:

```bash
cargo test --locked -p xtask --no-default-features \
  --features formal-claim-plan --bin formal-claim-plan
cargo clippy --locked -p xtask --no-default-features \
  --features formal-claim-plan --bin formal-claim-plan -- -D warnings
```

Produce a plan:

```bash
cargo run --locked -p xtask --no-default-features \
  --features formal-claim-plan --bin formal-claim-plan -- \
  --input path/to/formal-claim-selection-input.json --format json
```

`--format markdown` renders the same typed plan for review. Input is limited to
4 MiB. Malformed or contradictory input exits 2.

## Input contract

The draft schema is
[`schema/formal-claim-selection-input-v1.schema.json`](schema/formal-claim-selection-input-v1.schema.json).
The checked-in example is explicitly synthetic and non-authoritative.

The input contains:

1. repository identity;
2. a **claim-index projection**, not a second claim registry;
3. actual integration base `B` and merge group `G` declarations;
4. the complete ordered admitted PR/head membership;
5. expected registry, controller, producer, and proof-profile generations;
6. producer-owned changed paths, impact classes, and semantic subjects;
7. classification completeness and limitations.

The claim-index projection carries only fields selection needs. #14473 remains
the authority for the governed claim registry. The production integration must
generate this projection from that registry rather than maintain independent
hand-authored state.

The helper checks that the expected registry generation equals the supplied
projection generation. It does not authenticate either value, inspect Git, or
recompute the changed-path set.

## Selection law

A critical claim is directly selected when any supplied change matches:

- its production source closure;
- its claim/harness/model/runner authority paths; or
- one of its exact production owner subjects.

The plan falls back to **all registered critical claims** when:

- changed-code classification is incomplete;
- any change has unknown impact;
- no change facts are supplied;
- a global formal authority path changes; or
- a formal-relevant change maps to no registered claim.

Fallback never becomes zero work. A named completeness test must fail if a
critical claim is omitted from fallback.

A zero-selection plan is allowed only when supplied classification is complete,
change facts are nonempty, no fallback condition exists, and no changed path or
semantic subject matches a registered critical claim. The plan records a
human-readable zero-selection basis. This is supplied-fact consistency, not an
authenticated statement that `B..G` is semantically irrelevant.

Unregistered files cannot enter critical assurance because their names resemble
Kani, Verus, proofs, models, or tests. If the change producer says such a path is
formal relevant and no registered relation exists, the selector widens to all
critical claims instead.

## Phase and provider treatment

#14500 selects an admitted merge-group lifecycle. A native subject enters
`expected_native_subjects` only when all of these hold:

- the claim is critical;
- it is selected;
- its route is `native`;
- its provider is the currently admitted Kani profile; and
- its phase is `admitted_merge_group`.

Legacy `ordinary_pr_critical` and `merge_ready` values remain parseable for
historical inspection. They do not authorize a workflow start and their native
subjects are withheld from the admitted-G execution list. Scheduled, release,
manual, and unsupported obligations remain visible in the plan for their owning
route.

The helper creates no Draft, Ready, synchronize, label, push, or merge-group
workflow trigger. #14500 owns the eventual queue-only integration.

## Output contract

The draft output schema is
[`schema/formal-claim-plan-v1.schema.json`](schema/formal-claim-plan-v1.schema.json).
The plan binds:

- repository, `B`, `G`, and complete ordered member identities;
- claim-index, controller, producer, and proof-profile generations;
- raw input SHA-256;
- classification completeness and fallback reasons;
- every projected claim and its selected/not-selected disposition;
- route, phase, exact reasons, subjects, and limitations;
- deduplicated expected Kani subjects;
- explicit zero-selection basis when applicable;
- fixed non-authority and non-assurance fields.

Input ordering does not change claim ordering or semantic selection. Ordered
stack membership is intentionally not normalized because it is part of the
subject identity.

## Required controls

The focused Rust tests cover:

- exact production-owner and source-closure selection;
- harness/model-only authority changes;
- global-authority fallback;
- unknown and incomplete classification fallback;
- empty change-fact fallback;
- formal-relevant unmapped fallback;
- positively established zero selection;
- manual and unsupported route visibility;
- legacy native phase withholding;
- stale registry generation;
- invalid/duplicate stack identity;
- no-critical-claim refusal;
- unregistered lookalike paths;
- deterministic claim ordering;
- JSON/Markdown projection parity;
- duplicate keys, unknown fields, and missing required values;
- exact ordered stack identity retention.

Before this slice is accepted, run the focused tests and Clippy above, scoped
formatting, traceability, changelog, source policy, full current-source xtask
integration, and the applicable protected combined-tree qualification. Preserve
any missing or failed dimension; do not replace it with schema validation.

## Integration sequence

1. Land the architecture and offline evidence auditor from #14471/#14478 through
   their protected route.
2. Land this passive selector contract without adding a workflow trigger.
3. Implement #14473's governed claim registry and generate the claim-index
   projection from it.
4. Implement #14472's native Kani result producer and use one shared semantic
   validator at the runner/report boundary.
5. Join actual producer-owned `B/G/member/change` facts under #14500.
6. Execute selected Kani subjects advisably on the admitted-G route.
7. Measure selection completeness, latency, memory, disk, cache posture, and
   infrastructure outcomes.
8. Implement #6452/#6456 transition protection before any required-check
   admission under #12904.

## What this cannot establish

The helper cannot establish:

- that the supplied repository, Git objects, stack membership, registry,
  controller, producer, or change facts are authentic;
- that source closure or semantic owner mapping is complete;
- that a provider is installed, compatible, or executed;
- that a claim proposition matches intent or is correct;
- that a native result is current or successful;
- that a manual or unsupported obligation is discharged;
- that a proof may replace or retire base protection;
- that CI may start, pass, block, merge, or change branch protection.

Rollback removes this additive binary, schemas, examples, documentation, and
feature declaration. It changes no current Kani workflow, result, retained
artifact, branch protection, queue setting, or product support tier.
