# Fixture: ts_cross_module_reach_shadowed

Spec: RIPR-SPEC-0243

Issue: #13592

## Given

The discriminating twin of `ts_cross_module_reach_chain`: the same diff and
modules, but `normalize` takes a parameter named `decodeThing` and calls
that. The call reaches the parameter, not the import, so the changed
`decodeThing` of `src/a.ts` runs from no test.

## When

`ripr check --root input --diff ../diff.patch --format json --mode fast`
exercises the production CLI.

## Then

No cross-module edge is recorded (a shadowed name fails closed), so the
finding stays `no_static_path` with no related test.

## Must Not

- Credit a call of a rebound name to the imported helper.
