# Fixture: python_default_equal_keyword_no_exposed_credit

Spec: RIPR-SPEC-0028

## Given

`dehumanize` adds `check_imaginary=True` to its `current_time.shift(...)` call.
The only `shift` in the workspace declares `check_imaginary=True` as its
default, so the call binds the same value before and after. The only test
asserts `dehumanize(Clock(1), hours=2).t == 3`, which passes either way.

## When

`cargo xtask fixtures python_default_equal_keyword_no_exposed_credit`

## Then

The finding is not `exposed`. The added keyword restates the callee's default,
so the strong return-value assertion has no value change to tell apart, and
the finding reads `weakly_exposed` with that reason. This is the #16011
shape 2 false-exposed correction.

## Must Not

Credit the return-value assertion as observing a value the edit does not
change. The RIPR-SPEC-0108 honesty corpus independently forbids promotion.
