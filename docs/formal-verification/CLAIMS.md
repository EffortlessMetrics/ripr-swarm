# Formal claim registry contract

A formal harness or model is an implementation artifact. The semantic unit of
maintenance is a **registered claim**.

The initial implementation should use a versioned machine-readable registry
validated by repository policy. The exact self-assurance registry path and
generated Rust shape belong to #14473; the broader provider architecture stays
with #6381/#5198. The contract below is independent of that storage decision.

## Stable identity

A claim has:

```text
claim_id
claim_generation
```

`claim_id` identifies one proposition over one named production subject.
`claim_generation` changes whenever a decision-bearing field changes.

Renaming or relocating a harness does not require a generation change if the
proposition, production source closure, domain, assumptions, observable,
bounds, provider semantics, and result law are unchanged and the move is
recorded as an exact compatible replacement.

## Required fields

Every critical claim records:

| Field | Required meaning |
| --- | --- |
| `schema` | Exact `ripr.formal-claim.v1` serialization contract |
| `claim_id` | Stable descriptive identifier; never a bare function name |
| `claim_generation` | Positive integer for decision-bearing revisions |
| `family` | Bounded model checking, deductive proof, state-machine model, etc. |
| `provider` | Exact admitted native provider |
| `criticality` | Critical, important, advisory, scheduled, or manual |
| `phase` | Admitted merge group, scheduled, release, or manual; legacy ordinary-PR/merge-ready values are historical declarations only |
| `proposition` | Machine- and reviewer-readable statement of what is asserted |
| `non_claims` | Explicit statements the result does not establish |
| `owners` | Exact production symbols/files or a typed owner rule |
| `source_closure` | Code/config/model bytes whose movement invalidates result |
| `domain` | Symbolic or finite input domain |
| `assumptions` | Preconditions and modeled assumptions |
| `trust_dependencies` | Stubs, axioms, external bodies, admitted libraries |
| `observable` | Return, state, effect, ordering, reachability, or invariant |
| `bounds` | Unwind, state, schedule, width, depth, or campaign limits |
| `subjects` | Exact registered implementation locators |
| `required_covers` | Non-vacuity/reachability obligations |
| `negative_controls` | Deliberately wrong implementation or model controls |
| `result_satisfaction` | Native statuses required to establish the claim |
| `currentness_inputs` | Source, tool, config, provider, and registry identity |
| `transition_policy` | Base/head replacement and retirement behavior |
| `owner_spec` | Governing specification |
| `owner_issue` | Existing implementation/maintenance owner |

An unregistered harness may remain in the source census. It cannot increase
registered assurance coverage.

For the queue-only development lifecycle in #14500, new critical development
claims use `admitted_merge_group`. The document auditor retains the older
`ordinary_pr_critical` and `merge_ready` spellings to inspect historical input;
accepting either spelling does not authorize a workflow start. Phase selection
and source-bound native evidence belong to #14474 and the admitted-G plan,
not to the passive document audit.

## Claim families

The first self-assurance registry should group claims into:

1. classification and actionability;
2. gate, exit, and aggregate result semantics;
3. currentness, identity, and causal comparison;
4. resource and budget safety;
5. edit-cage, readiness, and repair completion;
6. bounded parsing or security-sensitive pure predicates where Kani remains
   economical.

The first user-facing registry should cover one bounded Kani relationship:

```text
proof harness
→ exact production owner
→ symbolic domain
→ asserted return/effect/observable
→ trust and assumption boundary
```

## Required covers

`cover!` is not decoration. A critical claim identifies the interesting
partitions whose reachability is required for the proof to be useful.
Every critical provider requires a nonempty `required_covers` obligation list.
For Kani, those obligations map to native `cover!` properties. For a state-machine
model, name the finite states/actions or transitions whose reachability must be
observed, and define their provider-native evidence mapping. An unsupported
mapping remains visible and unestablished; a zero-obligation list cannot stand
in for a provider-specific reachability contract.

Examples:

- actionable and non-actionable partitions;
- admitted and refused route;
- current and stale result;
- each terminal status class;
- both sides of a boundary predicate;
- zero and nonzero resource pressure where relevant.

A native “verified” result with an unreachable required cover is `FAILED` for
the registered claim. A cover not included in the registered claim is
advisory and must not silently affect claim status.

PR #14469 / #14460 owns the existing collector's complete-positive-cover
tightening. That collector treats every observed harness cover as required;
it does not yet implement registered advisory-cover distinctions. #14472 and
#14473 must compose with that behavior and make any changed cover policy an
explicit versioned admission change, with native controls.

## Negative controls

Every critical claim family has a discriminating negative control. Suitable
controls include:

- mutate one forbidden state to become actionable;
- map an unknown status to exit zero;
- accept a stale or duplicate identity;
- ignore a resource cap;
- bypass a readiness or edit-cage stop;
- remove a required assumption or observable;
- replace production code with copied logic in the harness.

The control must fail the named claim for the intended reason. A compile
failure, unrelated assertion failure, timeout, or missing subject does not
count as behavioral discrimination.

Negative controls may run on a bounded scheduled/canary route. Their exact
source, mutation, expected failing claim, and native outcome are retained.

## Source closure

The source closure includes every decision-bearing input to the proposition:

- production function or state machine;
- helper functions whose semantics are used;
- harness/model;
- assumptions and trusted stubs;
- relevant type/domain declarations;
- provider/tool configuration;
- feature/target/configuration predicates;
- model constants and bounds;
- result parser and claim satisfaction law where applicable.

A Git commit alone is insufficient for dirty/captured source. A same-HEAD edit
to any closure member invalidates affected results.

## Adding a claim

1. Identify the exact proposition and existing issue/spec owner.
2. Establish the production owner and source closure.
3. Select the evidence family; do not choose Kani merely because the code is
   Rust.
4. Add the registry row before counting the claim.
5. Add the harness/model with explicit bounds and assumptions.
6. Add meaningful covers.
7. Add or identify the negative control.
8. Run the exact provider and retain the machine result.
9. Add the claim to the assurance scoreboard.
10. Review nonclaims and transition behavior.
11. Only then consider CI phase promotion.

## Changing a claim

A decision-bearing change requires:

- a new generation;
- base/head authority comparison;
- current native results for the base-protective and head-added obligations;
- explicit scope-reduction, trust, assumption, bound, cover, or provider
  movement;
- refreshed negative controls where their source closure changed.

A head result cannot validate its own weakening of the base claim.

## Replacing or retiring a claim

Replacement requires:

- exact predecessor and successor IDs;
- proposition/domain/denominator comparison;
- complete successor coverage or explicit narrower nonclaim;
- current result for the successor;
- no orphaned critical owner.

Retirement requires typed evidence that the subject, feature, supported
platform, or product claim was removed or that an explicit governance decision
accepted the reduced protection.

Deleting the harness or registry row is not retirement evidence.

## Review checklist

A reviewer should be able to answer:

- What exact wrong behavior would falsify this proposition?
- Does the harness call production code?
- What inputs and states are outside scope?
- Which assumptions or trusted components can make the proof vacuous?
- Are the required covers reachable?
- Does the negative control fail the named claim?
- Which source/config changes invalidate the result?
- What is preserved from base authority during this PR?
- What does the result explicitly not claim?
