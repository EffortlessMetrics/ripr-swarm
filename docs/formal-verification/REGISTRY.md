# Governed formal-claim registry and assurance scoreboard

Owner: #14473 under #12109/#12904. Normative contracts:
[CLAIMS.md](CLAIMS.md), [RESULTS.md](RESULTS.md), and
[RIPR-SPEC-0371](../specs/RIPR-SPEC-0371-kani-bounded-model-checking-lane.md).

The registry governs ripr's self-assurance membership and generations. It does
not run Kani, authenticate evidence, establish currentness, authorize CI, or
promote declarations into assurance.

## Repository authority

```text
policy/formal-claims.json
policy/formal-claims.d/<stable-claim-id>.json
docs/formal-verification/schema/formal-claim-registry-v1.schema.json
docs/formal-verification/schema/formal-claim-index-v1.schema.json
docs/formal-verification/schema/formal-assurance-scoreboard-v1.schema.json
```

The manifest is the sole active-membership authority. Every regular claim JSON
must be listed exactly once; extra, missing, duplicate, malformed, non-JSON,
or symlinked entries refuse. Every directory component beneath the checkout
root and every final source/authority file is checked, including policy paths.

Claim semantics remain owned by `xtask/src/formal_evidence.rs`. The loader
snapshots bounded claim bytes, invokes that validator, requires a matching
bounded reread, and parses the snapshot. These checks refuse ordinary changes
and directory symlinks; they are not atomic custody or source currentness.
JSON and Markdown share typed values; Markdown escapes table/code/HTML/control
syntax. Path existence does not establish source-closure completeness.

## Commands

Use the pinned toolchain:

```bash
cargo run --locked -p xtask --no-default-features --features formal-claim-registry --bin formal-claim-registry -- check --registry policy/formal-claims.json
cargo run --locked -p xtask --no-default-features --features formal-claim-registry --bin formal-claim-registry -- index --registry policy/formal-claims.json --format json
cargo run --locked -p xtask --no-default-features --features formal-claim-registry --bin formal-claim-registry -- scoreboard --registry policy/formal-claims.json --format markdown
cargo run --locked -p xtask --no-default-features --features formal-claim-registry --bin formal-claim-registry -- transition --base-registry BASE/policy/formal-claims.json --head-registry HEAD/policy/formal-claims.json
```

Exit zero means internal consistency, never native proof success.

## Generations and retirement

The canonical decision digest commits claim semantics, locator, route, authority,
source closure, assumptions, bounds, observables, and required controls. Set-like
arrays sort; assumption order stays exact. The numeric claim generation is
outside that digest. Registry content commits repository identity, authorities,
active decisions/generations, and retirements, outside its numeric generation.

`expected_decision_digest` detects uncommitted decision edits. Base/head
`transition` additionally requires:

- changed decisions/content increase claim/registry generations;
- unchanged decisions/content retain their numeric generations;
- new claims start at generation 1;
- removal carries the exact final-generation retirement of an active base claim;
- replacements name another active claim;
- accepted retirements are immutable and cannot reactivate.

Replacement chains are not resolved: A naming B requires B to remain active.
Locator and wording changes follow the same generation law.

## Projection and baseline assurance

`index` emits `ripr.formal-claim-index.v1` for #14542, with registered identity,
source closure, authorities, subjects, and limitations. It always declares
`execution_authority = not_granted` and `native_assurance = not_assessed`.

`scoreboard` emits `ripr.formal-assurance-scoreboard.v1`: native/manual claims
remain `NOT_ESTABLISHED` without admitted evidence; unsupported routes remain
`UNSUPPORTED`. It has not joined native execution, negative controls, toolchain,
source commitments, merge membership, producer/controller/runner generations,
or accepted transitions. Future joins must reuse `formal_evidence` and #14500
lifecycle facts; historical outputs cannot be rebound as current passes.

The five initial claims cover classification, gate aggregation, currentness,
input bounds, and repair routing. Existing harnesses call production code and
declare reachability; their presence establishes no native assurance. Gate and
repair now include their domain definitions; those claims and the registry
advance to generation 2, while the other three claim generations remain 1.

## Qualification and maintenance

```bash
cargo test --locked -p xtask --no-default-features --features formal-claim-registry --lib
cargo clippy --locked -p xtask --no-default-features --features formal-claim-registry --lib --bin formal-claim-registry -- -D warnings
cargo build --locked -p xtask --no-default-features --features formal-claim-registry --bin formal-claim-registry
```

Validate schemas/examples, traceability, changelog, file policy, command catalog,
and actual outputs. Required controls cover exact membership, digest/duplicate
refusal, byte stability, generations/retirement, order independence, symlinked
directories, domain closure, Markdown escaping, and non-established assurance.
A claim change includes its document, manifest commitment, generation decision,
projection/example updates, negative controls, and base/head review.

Retain historical evidence and retired IDs. Rollback removes this additive
command, manifests, schemas, examples, and guide through the protected route;
existing harnesses, evidence, and owning issues remain. Removal proves nothing.
