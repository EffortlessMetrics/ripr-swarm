# Fixture: python_module_constant_boundary_test_local_control

Spec: RIPR-SPEC-0028
Spec: RIPR-SPEC-0108

## Given

The owner module `svc/a/src/pricing.py` defines the module constant
`DISCOUNT_THRESHOLD = 10_000` and `discounted_total`, whose discount
comparison reads it. The test `svc/a/tests/test_pricing.py` does `import pricing` and calls
`discounted_total(pricing.DISCOUNT_THRESHOLD)` with an exact-value
assertion, next to below- and above-threshold calls.
No other module named `pricing` exists in the workspace; this is the
unshadowed control for `python_module_constant_boundary_test_local_shadow`.

## When

The sole diff changes `amount > DISCOUNT_THRESHOLD` to
`amount >= DISCOUNT_THRESHOLD` on line 5 of `svc/a/src/pricing.py`.

## Then

Exactly one finding is reported for line 5, owned by
`python:svc/a/src/pricing.py::discounted_total`, classified `exposed`.
The attribute read binds the owner's boundary value, so the boundary call
discriminates the edit.

```expected-classes
probe:svc_a_src_pricing.py:python_preview:ecc92bfc -> exposed
```

## Must Not

Withhold the boundary credit when `pricing` identifies only the owner's
module; the shadow cases' non-promotion must come from the shadow, not from
losing the credit everywhere.
