# Fixture: python_raise_continuation_fstring_no_credit

Spec: RIPR-SPEC-0028
Spec: RIPR-SPEC-0108

## Given

`parse` changes the f-string message of a multi-line `raise ValueError(`
from `Invalid level: ...` to `Bad level: ...`; the message sits alone on a
continuation line. The unchanged tests assert exact upper-cased results for
inputs that never raise, and pin only the exception type with a bare
`pytest.raises(ValueError)`.

## When

`cargo xtask fixtures python_raise_continuation_fstring_no_credit`

## Then

The one finding is an error-path finding that stays `weakly_exposed`: the
continuation line takes the enclosing raise's error-path family, so the
success tests' exact-value assertions cannot observe the changed message and
the bare `pytest.raises` does not read it.

## Must Not

Report `exposed`. Before #15246 the line was classified alone as a predicate
and the success tests' exact assertions credited it. The governed
RIPR-SPEC-0108 honesty corpus independently forbids that promotion.
