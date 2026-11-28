# Fixture: python_package_alias_attribute_write

Spec: RIPR-SPEC-0238

## Given

`tests/__init__.py` binds `import tomli as tomllib`, then imports its own
package and assigns `tests.tomllib = json`. The test does
`from . import tomllib` and reads `tomllib.loads("")`. The package
initializer runs inside the package object Python already registered, so the
attribute write replaces the binding and the test receives `json`, not
`tomli` (#13927 review witness, confirmed on CPython).

## When

The diff changes the guard in `tomli._parser.loads`.

## Then

The changed owner has one finding and no related test. RIPR-SPEC-0238 rule 33
refuses an initializer in which the bound name appears as an attribute
anywhere, so the import keeps its unresolved reading.

## Must Not

Rewrite the test import to `import tomli as tomllib`, link `test_loads` to
`tomli.loads`, or promote the owner to `exposed`. RIPR-SPEC-0108 enforces this
independently of the generated goldens.
