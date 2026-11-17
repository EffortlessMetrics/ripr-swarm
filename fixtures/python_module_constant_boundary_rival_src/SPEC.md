# Fixture: python_module_constant_boundary_rival_src

Spec: RIPR-SPEC-0028
Spec: RIPR-SPEC-0108

## Given

The owner module `svc/a/src/pricing.py` defines the module constant
`DISCOUNT_THRESHOLD = 10_000` and `discounted_total`, whose discount
comparison reads it. The rival project's test `svc/b/tests/test_pricing.py` does `import pricing` and calls
`discounted_total(pricing.DISCOUNT_THRESHOLD)` with an exact-value
assertion, next to below- and above-threshold calls.
A rival src-layout module `svc/b/src/pricing.py` with a different
`DISCOUNT_THRESHOLD` is the `pricing` that project's tests import (#4566),
so `import pricing` in `svc/b/tests` does not identify the owner's module.

## When

The sole diff changes `amount > DISCOUNT_THRESHOLD` to
`amount >= DISCOUNT_THRESHOLD` on line 5 of `svc/a/src/pricing.py`.

## Then

Exactly one finding is reported for line 5, owned by
`python:svc/a/src/pricing.py::discounted_total`, classified `weakly_exposed`.
The attribute read through the rival `pricing` does not bind the owner's
boundary value (#14035), so the boundary call earns no boundary credit.

```expected-classes
probe:svc_a_src_pricing.py:python_preview:ecc92bfc -> weakly_exposed
```

## Must Not

Credit `pricing.DISCOUNT_THRESHOLD` as the owner's boundary value because the
module and attribute spellings match. Do not promote this finding to
`exposed`, erase it to read clean, or bless a promoted golden past the
independent honesty-corpus oracle.
