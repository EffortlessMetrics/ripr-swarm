# Fixture: python_package_alias_nested_root

Spec: RIPR-SPEC-0238

## Given

A nested project at `tests/project` whose own `tests` package binds
`import json as tomllib`, while the outer `tests/__init__.py` binds
`import tomli as tomllib`. The nested test package re-binds the name with
`from tests import tomllib as toml`, and its test reads `toml.loads("")`.
pytest imports the nested `tests` package from `tests/project`, so `toml` is
`json`, not `tomli` (#13927 review witness, confirmed on CPython 3.12).

## When

The diff changes the guard in `tomli._parser.loads`.

## Then

The changed owner has one finding and no related test. RIPR-SPEC-0238 rule 33
refuses the first hop already: its root-relative spelling
`tests.project.tests.unit` repeats `tests` on the test's path, where another
import root can carry `tests`, so the outer initializer is never read.

## Must Not

Follow the outer `tests/__init__.py`, link `test_loads` to `tomli.loads`, or
promote the owner to `exposed`. RIPR-SPEC-0108 enforces this independently of
the generated goldens.
