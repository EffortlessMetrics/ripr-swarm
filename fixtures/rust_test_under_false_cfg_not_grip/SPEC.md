# Fixture: rust_test_under_false_cfg_not_grip

Spec: RIPR-SPEC-0153

## Given

`price_with_tax` changes its tax divisor (`/ 10` → `/ 9`). The only test that
mentions it, `never_compiled`, sits under `#[cfg(any())]`, which is false in
every build, so the test never runs (#6293).

## When

```bash
ripr check \
  --root fixtures/rust_test_under_false_cfg_not_grip/input \
  --diff fixtures/rust_test_under_false_cfg_not_grip/diff.patch
```

## Then

The test is not discovered. Reach is `no` and the finding is `no_static_path`
(not `weakly_exposed`): no test that can run is seen calling `price_with_tax`.

## Must Not

- Credit `never_compiled` as reaching, observing or discriminating the change.
  Before this fixture the finding was `weakly_exposed` with that test named.
- Drop tests under a cfg ripr cannot evaluate (feature, target or custom
  atoms): those stay discovered, as before.

## Known limits

Not covered here, and stated so the fixture is not read as a broader claim:
this fixture runs on the parser-backed path and pins only `cfg(any())`. The
lexical fallback's former 32-line window and comment-between misses are pinned
by the `test_styles` lexical controls (#7043). An unclosed `#[`, or a gate past
that scan's byte budget, still fails open.
