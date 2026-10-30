# Fixture: rust_qualified_helper_effect_reach

Spec: RIPR-SPEC-0159

## Given

A changed private helper pushes a value into caller-owned state. The test
calls a wrapper that reaches the helper through `crate::record(out)` and
asserts the resulting vector.

## When

`ripr check` analyzes the changed effect and call-deletion probes.

## Then

Both findings relate the test as `helper_owner_call` but remain
`propagation_unknown`, with `helper_result_not_forwarded`.

## Must Not

Treat the qualified-hop identity receipt as effect propagation or promote
either finding to `exposed`. The ordinary bare-call effect control retains
its existing propagation; only the new qualified route is reach-only.
