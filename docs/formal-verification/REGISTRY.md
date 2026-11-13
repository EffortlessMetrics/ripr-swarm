# Governed formal-claim registry and assurance scoreboard

Owner: #14473 under #12109/#12904. Normative contracts:
[CLAIMS.md](CLAIMS.md), [RESULTS.md](RESULTS.md), and
[RIPR-SPEC-0371](../specs/RIPR-SPEC-0371-kani-bounded-model-checking-lane.md).

This is ripr's canonical self-assurance membership/generation layer. It does
not run Kani, authenticate evidence, establish currentness, authorize CI, or
promote supplied declarations into assurance.

## Repository authority

```text
policy/formal-claims.json
policy/formal-claims.d/<stable-claim-id>.json
docs/formal-verification/schema/formal-claim-registry-v1.schema.json
docs/formal-verification/schema/formal-claim-index-v1.schema.json
docs/formal-verification/schema/formal-assurance-scoreboard-v1.schema.json
```

`policy/formal-claims.json` is the sole active-membership authority. Every
regular `.json` file in `policy/formal-claims.d` must be listed exactly once.
Extra, missing, symlinked, duplicate, malformed, or non-JSON entries refuse.

Claim semantics remain owned by `xtask/src/formal_evidence.rs`. The registry
snapshots each regular claim file, invokes that shared path-based validator,
then requires a bounded reread to equal the snapshot and parses the snapshotted
bytes. This refuses ordinary concurrent edits/path swaps; it is not an atomic
filesystem transaction, authenticated custody, or evidence of verifier inputs.

Every declared source-closure and authority path must resolve to a regular
non-symlink file. Existence does not establish completeness or currentness.
JSON and Markdown share typed values; Markdown escapes table/code/HTML/control
syntax, but projection safety does not authenticate declarations.

Every directory component below the supplied checkout root is checked as well
as the final file, including `policy/` and `policy/formal-claims.d/`. A directory
symlink cannot redirect a claim, source closure or authority path outside that
root. These checks inspect local paths; they do not provide an immutable
filesystem snapshot or authenticate a concurrently modified checkout.

## Commands

Using the pinned toolchain:

```bash
cargo run --locked -p xtask --no-default-features --features formal-claim-registry --bin formal-claim-registry -- check --registry policy/formal-claims.json
cargo run --locked -p xtask --no-default-features --features formal-claim-registry --bin formal-claim-registry -- index --registry policy/formal-claims.json --format json
cargo run --locked -p xtask --no-default-features --features formal-claim-registry --bin formal-claim-registry -- scoreboard --registry policy/formal-claims.json --format markdown
cargo run --locked -p xtask --no-default-features --features formal-claim-registry --bin formal-claim-registry -- transition --base-registry BASE/policy/formal-claims.json --head-registry HEAD/policy/formal-claims.json
```

Exit zero means deterministic internal consistency, never native proof success.

## Decision and registry generations

Each active row commits one claim generation to a canonical decision digest
covering:

- claim ID, document locator, route, and authority paths;
- family/provider/criticality/phase, proposition, and nonclaims;
- owners/source closure, domain, assumptions, and trust;
- observable, bounds, subjects, required covers, and negative controls;
- result/currentness/transition law, owner spec, and owner issue.

Set-like arrays are sorted. Assumption order remains exact because native
results bind the compact assumptions array. `claim_generation` is outside the
decision digest: changed decisions must increase it; unchanged decisions retain
it. The manifest's `expected_decision_digest` detects uncommitted current-tree
changes, while `transition` detects matching claim+manifest edits without a
generation increase.

Registry identity is:

```text
<registry_generation>:sha256:<canonical-registry-content>
```

The content commits repository identity, global/per-claim authority, active
documents, claim IDs/generations/routes/digests, and retirements; it excludes
the numeric registry generation itself.

Base/head transition validation requires:

- unchanged content retains `registry_generation`; changed content increases it;
- unchanged decisions retain `claim_generation`; changed decisions increase it;
- new claims start at generation 1;
- removed claims carry their exact final-generation retirement;
- a new retirement corresponds to an active base claim removed in that change;
- replacements name another active claim;
- accepted retirements are append-only, immutable, and cannot reactivate.

Replacement chains are not resolved: if A names B, B must stay active;
A→B→C requires a future explicit policy change.

Locator or wording changes are not assumed compatible. Record the appropriate
generation or explicit retirement/replacement.

## Planner projection and baseline scoreboard

`index` emits `ripr.formal-claim-index.v1` for #14542, containing registry
generation, global authority paths, claim identity/generation, criticality,
phase, provider/route, owners/source closure, authority paths, subjects, and
limitations. It always fixes:

```text
execution_authority = not_granted
native_assurance = not_assessed
```

#14500 supplies authenticated B/G/member/change/controller/producer facts; the
registry does not rediscover them.

`scoreboard` emits `ripr.formal-assurance-scoreboard.v1`. Without an admitted
result, native/manual claims are `NOT_ESTABLISHED` and unsupported routes are
`UNSUPPORTED`. It explicitly has not joined native execution, negative
controls, provider/toolchain/configuration, source/worktree commitments,
merge-group membership, runner/controller/producer generations, or transition
acceptance. A later join must reuse `formal_evidence`; it cannot reinterpret
this baseline or historical output as a pass.

The initial critical population covers:

1. classification/actionability;
2. gate/result aggregation;
3. source currentness/candidate-action identity;
4. bounded input/resource arithmetic;
5. repair-route fail-closed exposure.

Each existing Kani harness calls production code and declares reachability.
Their registry-only status remains `NOT_ESTABLISHED`; harness presence and old
Kani runs are not rebound to this generation.

Generation 2 added the production domain definitions to the gate and repair
claims and increased both claim generations to 2.

Generation 3 increases the currentness claim to generation 2. Its required
covers now name all four `SourceCurrentness` constructors and both existing
semantic predicates. The unchanged harness emits all six native properties;
each obligation must map to a distinct property. The proposition, source
domain, bounds, subject, and the other four claim generations are unchanged.
Earlier native receipts remain bound to their original claim and registry
generations. This declaration does not establish a native result.

Generation 4 increases classification to generation 2 and gate/repair to
generation 3. Their symbolic selectors now use the actual lengths of the
compiler-checked constructor domains in `crates/ripr/src/proof_domains.rs`,
which is included in all three source closures. Required covers are the five
EvidenceState constructors plus two actionability predicates, four GateMode
constructors plus five statuses, and five FixInstructionState constructors
plus three route predicates: 7, 9, and 8 distinct obligations respectively.
Each constructor obligation requires its own native property, including the
macro invocation location and exact constructor description; shared source
lines do not make properties interchangeable.

The gate proof retains its zero/one empty-String input domain and unwind 16,
using immutable slices because production only observes emptiness. Its claim
explicitly excludes allocation, vector destruction, String contents and
addresses. The production predicates and propositions are unchanged. This
transition leaves the currentness and resource claim documents unchanged.

Ordinary exact-item constructor controls and isolated native positive,
intended-production-mutant and restoration checks are qualification evidence
for their recorded source subjects. A range-omission control demonstrates that
native process success can coexist with an unsatisfied constructor cover;
required-cover admission must still reject that result. These experiments do
not establish full-package/default-profile assurance or rebind historical
receipts. Registry-only scoreboards continue to report five
`NOT_ESTABLISHED` claims until native binding, configuration, source and custody
admission is complete.

Registry generation changes apply to every native binding. The 3-to-4 transition
requires re-preparation of bindings and capture/replay configuration against the
new registry identity, including the unchanged currentness and resource claim
decisions. Matching a claim ID, proposition or decision digest alone does not
make an older preparation or receipt current.

## Qualification

Focused commands:

```bash
cargo test --locked -p xtask --no-default-features --features formal-claim-registry --lib
cargo clippy --locked -p xtask --no-default-features --features formal-claim-registry --lib --bin formal-claim-registry -- -D warnings
cargo build --locked -p xtask --no-default-features --features formal-claim-registry --bin formal-claim-registry
```

The diagnostic Rustfmt workflow retains exact patch and file-list artifacts,
including empty outputs for a clean checkout. Native formatter failures still
fail the diagnostic. Its measured nine-line shell budget is explicit in
`policy/workflow_allowlist.txt`; it grants no proof or merge authority.

Also validate all schemas/examples, traceability, changelog, source/file policy,
command catalog, and actual generated outputs. Required discriminators include:

- unlisted/listed-missing claim files, duplicate keys, and digest mismatch;
- claim bytes changing during admission;
- claim/registry changes without generation increases;
- removal without retirement, absent replacement, invented retirement, and
  retired-claim reactivation;
- unchanged decisions/registries cannot change numeric generations;
- new claims start at generation 1 and retirements retain the exact final generation;
- accepted retirement records cannot be removed or rewritten;
- symlinked source/authority/policy directories and missing domain definitions;
- order-only changes preserving identity;
- unsafe Markdown syntax;
- a registry-only scoreboard that never reports established assurance.

## Native join boundary

A #14472 result contributes only when all of these agree:

```text
claim ID/generation and decision/registry generation
registered subject and complete selected source closure
provider, solver, Rust toolchain, configuration, assumptions, and bounds
selection plan, base/group, and ordered admitted member heads
runner, controller, and producer generations
normal process/cleanup, required covers, and native row outcome
```

Missing, abnormal, stale, duplicate, unsupported, or ambiguous evidence remains
visible and non-success. Required-status promotion also needs authoritative
#14500 lifecycle facts and #6452/#6456 transition protection.

## Maintenance and rollback

A claim change includes its document, manifest commitment, generation decision,
projection/example updates, negative-control disposition, and base/head review.
Retain historical artifacts; never rewrite old evidence as current, delete a
failure to turn the scoreboard green, or reuse a retired claim ID.

Rollback removes this additive registry command, manifest, claim documents,
schemas, examples, and runbook through the protected route. Existing harnesses,
evidence, and owning issues remain; removal establishes nothing.
