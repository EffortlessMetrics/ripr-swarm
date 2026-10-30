# Fixture: ts_class_getter_factory_read

Spec: RIPR-SPEC-0243

Issue: #10402

## Given

`Cached.value` is a getter. The module exports `cached(getter)`, a function
whose only `return` is `new Cached(getter)`. The diff flips the getter's
`getter !== undefined` test to `getter === undefined`. The test imports
`cached`, binds `const box = cached(() => ++runs)` once and asserts
`expect(box.value).toBe(1)`.

## When

`ripr check --root input --diff ../diff.patch --format json --mode fast`
exercises the production CLI.

## Then

The getter is related to the test: reading `box.value` runs the getter of the
class the factory built, so reach is `yes` and the finding is no longer
`no_static_path`. The changed predicate boundary is still not discriminated, so
the class stays `weakly_exposed` (RIPR-SPEC-0243 decision 23).

## Must Not

- Report `no_static_path` for a getter whose test reads it on a factory-built
  instance.
- Promote the finding to `exposed` on factory reach alone.
