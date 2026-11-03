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
`legacy` and the rest to empty, so the existing caller (which passes runner and
routing configuration but none of the admission inputs) keeps its behavior. Source branch deviation: it declared
`proof-profile` required; that would break the current caller, so it is optional
here.

The checkout resolves `inputs.subject-sha || github.sha`. A new
`Verify admitted integration before proof` step runs before any proof:

- `legacy` rejects any partial qualification input and publishes no outputs, so
  PR evidence and the summary keep exactly the original event expressions
  (`pull_request`, `merge_group` and `push` behave as on main).
- `qualification` requires 40-hex head/base/merge/tree/generation, a 64-hex
  identity, a caller-supplied constituent (positive PR number, non-empty PR node
  id and exact head equal to `head-sha`, plus a non-empty repository node id), generation equal to `GITHUB_WORKFLOW_SHA`, subject JSON (version 2)
  matching repository, PR number and node id, destination, head, base, merge, tree, generation and
  `parents == [base, head]`, a matching canonical digest, and a checked-out
  commit whose id, tree and parents equal the admitted ones.
- `legacy` also rejects any supplied constituent input.
- Any other profile fails closed.

The step never reads `github.event.pull_request.*`: a `merge_group` event has no
pull request object. The constituent identity is data the caller (not candidate
code) must pass; missing, empty, contradictory, stale or wrong-head values fail
inside the verification step, before scratch or any product allocation.

PR evidence and the gate summary prefer the admitted head/base and record the
integration subject and tree. The main-only nextest archive measurement (#11594)
is preserved.

## Evidence and limitations

`rust_gates_admission.rs` is a source-shape check (optional inputs, ordering,
required predicates, preserved main behavior, no pull_request payload read).
`rust_gates_admission_replay.rs` extracts the step's Bash body and runs it in a
merge-group-shaped environment (cleared environment, no PR payload fields,
private Git repository with a real `[base, head]` merge commit): a valid tuple
passes and publishes identities; missing, empty, wrong or stale PR number, node
id, constituent head, repository id, head, base, merge, tree and generation,
reversed parents (in the JSON and in the checked-out commit), malformed or
mismatched subject JSON and identity digest all fail closed with no outputs;
`legacy` stays inert. Remaining caller dependency: no caller passes these inputs
yet; the later admission slice must supply the trusted tuple. Hosted CI is the
arbiter.

## Rollback

Revert the change; the inputs are additive and unused by the current caller.
