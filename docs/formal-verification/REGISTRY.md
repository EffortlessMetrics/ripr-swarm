# Governed formal-claim registry and assurance scoreboard

Owner: #14473 under #12109 / #12904. Normative claim and result contracts:
[CLAIMS.md](CLAIMS.md), [RESULTS.md](RESULTS.md), and
[RIPR-SPEC-0371](../specs/RIPR-SPEC-0371-kani-bounded-model-checking-lane.md).

This is the canonical membership and generation layer for ripr self-assurance.
It does not run Kani, authenticate a proof artifact, establish source
currentness, authorize CI, or promote a supplied declaration into assurance.

## Repository layout

```text
policy/formal-claims.json
policy/formal-claims.d/<stable-claim-id>.json
docs/formal-verification/schema/formal-claim-registry-v1.schema.json
docs/formal-verification/schema/formal-claim-index-v1.schema.json
docs/formal-verification/schema/formal-assurance-scoreboard-v1.schema.json
```

`policy/formal-claims.json` is the only active-membership authority. Every
regular `.json` file in `policy/formal-claims.d` must be listed exactly once,
and every listed file must exist. Extra, missing, symlinked, duplicated, or
non-JSON entries refuse the registry.

Claim-document semantics remain owned by `xtask/src/formal_evidence.rs`. The
registry snapshots each listed regular claim file, invokes that shared
path-based validator, and then requires a bounded reread to match the exact
snapshotted bytes consumed by the registry. This refuses an ordinary concurrent
edit or path swap between semantic admission and registry projection. It is a
stability observation, not an atomic filesystem transaction, authenticated
source custody, or proof of bytes consumed by an external verifier.

The registry also requires every declared source-closure and authority path to
resolve to a regular non-symlink file in the checkout. Existence does not
establish completeness or currentness. The registry does not maintain a second
positive-evidence admission law.

All Markdown projections escape table separators, code delimiters, HTML syntax,
and control characters. JSON and Markdown derive from the same typed values;
projection safety does not authenticate the underlying declarations.

## Commands

Use the repository-pinned toolchain:

```bash
cargo run --locked -p xtask --no-default-features \
  --features formal-claim-registry --bin formal-claim-registry -- \
  check --registry policy/formal-claims.json

cargo run --locked -p xtask --no-default-features \
  --features formal-claim-registry --bin formal-claim-registry -- \
  index --registry policy/formal-claims.json --format json

cargo run --locked -p xtask --no-default-features \
  --features formal-claim-registry --bin formal-claim-registry -- \
  scoreboard --registry policy/formal-claims.json --format markdown
```

Compare a base and head registry before accepting a claim-policy transition:

```bash
cargo run --locked -p xtask --no-default-features \
  --features formal-claim-registry --bin formal-claim-registry -- \
  transition \
  --base-registry BASE/policy/formal-claims.json \
  --head-registry HEAD/policy/formal-claims.json
```

Exit zero means the registry operation was deterministic and internally
consistent. It never means a native proposition was established.

## Decision digest

Each active manifest entry commits one claim generation to a canonical
decision digest. The digest includes:

- stable claim ID and claim-document locator;
- native, manual, or unsupported route and per-claim authority paths;
- family, provider, criticality, and phase;
- proposition and explicit nonclaims;
- owners and complete source closure;
- domain, assumptions, and trust dependencies;
- observable and bounds;
- subjects and required covers;
- negative controls;
- result-satisfaction and currentness laws;
- transition policy, owner spec, and owner issue.

Set-like string arrays are sorted before hashing. Assumption order is preserved
because the native result contract binds the exact compact assumptions array.
`claim_generation` is intentionally outside the decision digest: a changed
decision must increase the generation, while an unchanged decision must retain
it.

The manifest stores `expected_decision_digest`. Changing a claim without
updating that commitment fails the current registry. Updating both without a
generation bump is caught by the base/head `transition` command.

## Registry generation

The generated registry identity is:

```text
<registry_generation>:sha256:<canonical-registry-content>
```

The content commitment includes repository identity, global authority paths,
active document paths, claim IDs and generations, routes, per-claim authority
paths, decision digests, and retirement records. It excludes the numeric
registry generation itself.

Transition validation requires:

- unchanged content retains `registry_generation`;
- any active claim, authority, route, membership, or retirement change
  increases `registry_generation`;
- a changed claim decision increases its `claim_generation`;
- an unchanged claim decision cannot change its generation;
- a new claim starts at generation 1;
- a removed claim has an explicit retirement with its exact final generation;
- every newly added retirement corresponds to an active base claim removed by
  that transition—an unrelated tombstone cannot be invented;
- a replacement names another active claim;
- accepted retirements are append-only and cannot be rewritten or reactivated.

A locator or wording change is not silently treated as compatible. The
maintainer must decide whether it changes the registered proposition or its
source/currentness boundary, then record the appropriate generation or
retirement transition.

## Planner projection

`index` emits `ripr.formal-claim-index.v1`. Its `claim_index` object is the
governed projection consumed by the passive planner in #14542:

- exact registry generation;
- global authority paths;
- claim ID and generation;
- criticality and phase;
- provider and route;
- owners and source closure;
- per-claim authority paths;
- native/manual subjects;
- explicit limitations copied from the claim's nonclaims.

The projection always fixes:

```text
execution_authority = not_granted
native_assurance = not_assessed
```

The admitted merge-group lifecycle under #14500 supplies authenticated
`B/G/member/change/controller/producer` facts. The registry does not inspect or
rediscover those facts.

## Registry-only scoreboard

`scoreboard` emits `ripr.formal-assurance-scoreboard.v1`. In this first bounded
slice no native result is accepted, so each native or manual claim remains
`NOT_ESTABLISHED`; an unsupported route remains `UNSUPPORTED`.

The registry-only scoreboard explicitly records that it has not joined:

- an exact admitted native run;
- negative-control execution;
- provider, solver, Rust toolchain, or effective configuration identity;
- source-closure and worktree commitments;
- actual merge-group and ordered membership;
- runner/controller/producer generations;
- base/head transition acceptance.

This baseline is useful immediately: every critical obligation is visible
before the #14472 native-result producer and #14500 lifecycle can supply an
admitted join. A later result-join slice must reuse the shared
`formal_evidence` predicates and must not reinterpret the registry-only output
as a historical pass.

## Initial critical claims

The first registry population deliberately covers five load-bearing families:

1. classification/actionability;
2. gate and result aggregation;
3. source currentness and candidate-action identity;
4. bounded input/resource arithmetic;
5. repair-route fail-closed exposure.

Each claim calls production code from an existing Kani harness and declares
required reachability. Their current registry-only scoreboard remains
`NOT_ESTABLISHED`; source presence and prior unrelated Kani runs are not
silently rebound to this generation.

## Qualification

Focused qualification for this slice:

```bash
cargo test --locked -p xtask --no-default-features \
  --features formal-claim-registry --lib formal_registry::

cargo clippy --locked -p xtask --no-default-features \
  --features formal-claim-registry \
  --lib --bin formal-claim-registry -- -D warnings

cargo build --locked -p xtask --no-default-features \
  --features formal-claim-registry --bin formal-claim-registry
```

Also validate all three schemas and checked-in examples, traceability,
changelog, source/file policy, command catalog, and the actual generated
outputs from `policy/formal-claims.json`.

Required discriminators include:

- an unlisted claim file;
- a listed missing file;
- duplicate manifest keys;
- decision-digest mismatch;
- claim bytes changing while semantic admission is in progress;
- decision change without claim-generation increase;
- registry change without registry-generation increase;
- removal without retirement;
- a newly invented retirement with no active base authority;
- retirement with an absent replacement;
- reactivation of a retired claim;
- output-order changes that do not change generation;
- Markdown control/table syntax in projected text;
- a registry-only scoreboard that never reports established assurance.

## Native join boundary

Do not join a #14472 result until all of these identities agree:

```text
claim ID and generation
decision and registry generation
registered subject
complete selected source closure
provider, solver, Rust toolchain and configuration
assumptions and bounds
selection-plan identity
base and actual merge group
ordered admitted member heads
runner, controller and producer generations
normal process and cleanup state
required covers and native row result
```

Missing, abnormal, stale, duplicate, unsupported, or ambiguous evidence remains
visible and non-success. The join must consume authoritative lifecycle facts
from #14500 and transition protection from #6452/#6456 before any required
status promotion.

## Maintenance and rollback

A claim change must include the claim document, manifest commitment, generation
decision, affected projection/example updates, negative-control disposition,
and review of base/head transition behavior.

Retain historical claim/result artifacts. Do not edit an old artifact to look
current, delete a failed result to make the scoreboard green, or reuse a claim
ID after retirement.

Rollback removes this additive registry command, manifest, claim documents,
schemas, examples, and runbook through the protected route. Existing Kani
harnesses and historical evidence remain intact; removing the registry does
not convert any claim into established or erase its owning issues.
