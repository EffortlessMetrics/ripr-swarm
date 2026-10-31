# Fixture: rust_verification_contract_only_edit

Spec: RIPR-SPEC-0368

## Given

A Rust crate whose `bump` carries a `#[requires(x > 0)]` contract that the diff
tightens to `x > 1`. The body and the test are untouched.

## When

```bash
cargo xtask fixtures rust_verification_contract_only_edit
```

or:

```bash
ripr check --root fixtures/rust_verification_contract_only_edit/input --diff fixtures/rust_verification_contract_only_edit/diff.patch --mode fast
```

## Then

`ripr` should report `no_behavioral_candidates` with no findings and one
`verification_only_change` limitation that names the changed contract and tells
the reader to re-run the verifier.

## Must Not

- Report the contract change as silence (an empty `limitations` list).
- Emit any finding or repair start for the contract line.
- Evaluate, credit or weigh the contract clause, or claim a proof result.
