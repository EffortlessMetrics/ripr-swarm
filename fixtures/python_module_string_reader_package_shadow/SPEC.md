# Fixture: python_module_string_reader_package_shadow

Spec: RIPR-SPEC-0028
Spec: RIPR-SPEC-0108

## Given

The production module `fx/core.py` defines `GREETING` and its string reader.
A competing regular package under `tests/fx` supplies `tests/fx/core/__init__.py` with an
unrelated `greet` that always returns `'hi, bob'`. The standalone test imports
`greet` from `fx.core` and has an unchanged membership assertion `'bob' in greet('bob')`.
Pytest's default prepend import order can load the test-root package before
the production package. CPython with those roots imports the competing reader
and returns `'hi, bob'` both before and after the production edit.

## When

The sole diff changes the production `GREETING` from `'hello'` to `'hi'`.
Neither the competing reader nor the test changes.

## Then

Exactly one `static_unknown` finding retains unknown reach. The changed
production module has no established import identity, so the test supplies no
strong credit, actionable weak observation, repair packet or receipt command.
The ordinary app regression separately preserves the canonical configured
`src` layout and checks removal of the competing package restores its route.

## Must Not

Treat the matching import spelling or the asserted value as proof that the
changed production reader executes. Do not lend the competing reader's
assertion to the production edit, erase the finding to claim clean output,
or bless a promoted golden to bypass the independent honesty-corpus oracle.
CPython import observations are not pytest or runtime-mutation qualification.
