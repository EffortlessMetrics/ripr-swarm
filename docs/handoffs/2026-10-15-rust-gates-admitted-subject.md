# Rust gates admitted-subject leaf (#4987)

## Scope

This leaf changes only `.github/workflows/rust-gates.yml`, adds its static
contract (`tools/repo-policy/tests/rust_gates_admission.rs`) and this note. The
exact-subject `ci.yml` leaf landed earlier; this is the reusable gate it and a
later routed caller will invoke. No caller is added or changed and nothing is
activated.

## Contract

The `workflow_call` interface gains `proof-profile` plus `head-sha`, `base-sha`,
`subject-sha`, `subject-tree`, `workflow-generation`, `subject-json` and
`subject-identity`. Every input is optional: `proof-profile` defaults to
`legacy` and the rest to empty, so the existing caller (which passes only
`runner-config`) keeps its behavior. Source branch deviation: it declared
`proof-profile` required; that would break the current caller, so it is optional
here.

The checkout resolves `inputs.subject-sha || github.sha`. A new
`Verify admitted integration before proof` step runs before any proof:

- `legacy` rejects any partial qualification input and publishes the legacy
  event head/base.
- `qualification` requires 40-hex head/base/merge/tree/generation, a 64-hex
  identity, generation equal to `GITHUB_WORKFLOW_SHA`, subject JSON (version 2)
  matching repository, PR, destination, head, base, merge, tree, generation and
  `parents == [base, head]`, a matching canonical digest, and a checked-out
  commit whose id, tree and parents equal the admitted ones.
- Any other profile fails closed.

PR evidence and the gate summary prefer the admitted head/base and record the
integration subject and tree. The main-only nextest archive measurement (#11594)
is preserved.

## Evidence and limitations

The test is a source-shape check (optional inputs, verification ordering and
required predicates, admitted identities, preserved main behavior). It does not
replay the Bash body, run the hosted gate or exercise a caller; behavioral replay
belongs to the later admission slice. Hosted CI is the arbiter.

## Rollback

Revert the change; the inputs are additive and unused by the current caller.
