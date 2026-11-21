# Fixture: python_patched_callee_reachable_unrevealed

Spec: RIPR-SPEC-0028
Spec: RIPR-SPEC-0108

## Given

A Python module `pricing.py` defines `rate_for(region)` (returning `3` after
the change) and its caller `tax_for(amount, region)`, which returns
`amount * rate_for(region)`. The only related test patches the changed callee
itself at the top of its body with
`monkeypatch.setattr("pricing.rate_for", lambda region: 2)` and then asserts
`tax_for(10, "x") == 20` against the stand-in's value.

## When

The return value of `rate_for` changes from `1` to `3`:

```bash
ripr check \
  --root fixtures/python_patched_callee_reachable_unrevealed/input \
  --diff fixtures/python_patched_callee_reachable_unrevealed/diff.patch \
  --mode draft
```

## Then

One `reachable_unrevealed` finding for `rate_for`: every related test replaces
the changed function itself (`owner_patched_by_every_test` in
`crates/ripr/src/analysis/language/python/classify.rs`, via
`callee_patch.rs`), so the tests reach its caller but none of them runs the
changed code and nothing reveals the edit. This is the first standalone Python
golden fixture pinning `reachable_unrevealed` (previously covered only by unit
tests in `callee_patch_tests.rs`).

## Must Not

- Credit the stand-in assertion (`tax_for(10, "x") == 20` under the patch) as a
  discriminator for the real `rate_for` change.
- Report `exposed`, `weakly_exposed`, or `static_unknown` for this change: the
  test does reach the owner's caller, so it is a concrete reachable gap, not an
  unknown.
- Claim a runtime mutation outcome from static evidence.
