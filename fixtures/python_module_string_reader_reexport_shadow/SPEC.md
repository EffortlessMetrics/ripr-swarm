# Fixture: python_module_string_reader_reexport_shadow

Spec: RIPR-SPEC-0028
Spec: RIPR-SPEC-0108

## Given

The production module `fx/core.py` defines `GREETING` and its string reader.
`wrapper/__init__.py` re-exports that reader, but `tests/wrapper.py` defines
an unrelated `greet` that always returns `'hi, bob'`. The standalone test
imports `greet` from `wrapper` and pins `'hi, bob'`. Under pytest's default
prepend import order, the test-root module can replace the re-export package.
CPython with those roots loads `tests/wrapper.py` and the assertion passes
before and after the production edit.

## When

The sole diff changes the production `GREETING` from `'hello'` to `'hi'`.
The re-export package, competing module and test remain unchanged.

## Then

Exactly one `static_unknown` finding retains unknown reach, with no repair
packet or receipt command. A fact establishing `fx.core` import identity
cannot establish the separate `wrapper` import used by this test. The
bounded string reader accepts only checked direct owner-module spellings.

## Must Not

Use package re-export metadata to infer that the asserted function is the
changed production reader. Do not promote this oracle, erase the finding
to claim clean output, or let an expected-output re-bless override the
independent honesty-corpus refusal. The CPython observation does not claim
pytest execution or runtime mutation qualification.
