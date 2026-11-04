Spec: RIPR-SPEC-0147

# Incomplete diff corpus (#4514)

Parent qualification leaf: #4506. Shared exact-candidate harness: #4510.

## Given

PR #4439 landed declared-span accounting on `main`. Scattered unit and CLI
smoke tests cover slices of the behavior, but #4514 requires a versioned
corpus with exact byte digests and a runner that exercises both public diff
routes for every retained row.

## When

`cargo test -p ripr --test incomplete_diff_corpus` loads `corpus.json`,
recomputes each diff digest, and runs `ripr check --diff <file>` and
`ripr check --diff -` against `fixtures/boundary_gap/input`.

## Then

- Every incomplete row stays `analysis_complete=false` with a typed
  `malformed_diff` limitation at `diff_parse` and a retry recovery route.
- Complete controls stay complete on both routes with matching file/stdin state.
- Human stdout must not strengthen an incomplete JSON outcome.
- `cargo xtask check-fixture-contracts` validates corpus structure and digests.

## Must Not

- Do not claim exact-candidate qualification, #2769 packet emission, removal
  controls, badge/gate consumer parity, or ordinary-fixture declared-count
  validation for the whole workspace; those remain open #4514 acceptance items.
- Do not close #4514 from this corpus alone.

## Proof

```bash
cargo test -p ripr --test incomplete_diff_corpus
cargo xtask check-fixture-contracts
```
