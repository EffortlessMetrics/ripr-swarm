# Fixture: python_parent_collected_test_shadow

Spec: RIPR-SPEC-0028

## Given

A project `svc/a` has `core.py` with `price(x)`. Its `tests` directory (not a
package) holds a helper `core.py` whose `price` always returns `3`, and a test
module pytest collects, `test_a.py`. The non-package subdirectory
`tests/unit` holds `test_x.py`, which does `from core import price` and asserts
`price(2) == 3`.

## When

The diff changes the project's `price` from `x + 1` to `x + 2`.
The fixture input is the after-edit tree; its test is unchanged.

## Then

The changed `price` has one finding that is not `exposed`. A pytest run that
collects `tests` imports `test_a.py` first, which puts `svc/a/tests` on
`sys.path`, so `test_x.py` imports the helper `core` and both versions of the
project's `price` pass its assertion (#14083).

## Must Not

Promote `price` to `exposed` through `test_x.py`'s strong assertion, or report a
clean result. The shadowed import names the helper, not the project module. RIPR-SPEC-0108 independently enforces the finding count and
`must_not_promote`, even if the generated goldens are re-blessed.
