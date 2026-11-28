# Classifier opaque-signal disclosure baseline

Spec: RIPR-SPEC-0009

## Given

Seven changed comparison boundaries have separately related tests. Three test
bodies independently mention `fixture`, `builder`, or `arb_`; three changed
expressions independently mention `async`, `spawn`, or `await`; one plain
boundary and test mention none of these signals.

## When

Run `cargo xtask fixtures classify_opaque_signals`.

## Then

The committed JSON pins each finding's classification and stop reasons on the
pre-refactor classifier. The three body signals disclose `fixture_opaque`; the
three expression signals disclose `async_boundary_opaque`; the plain case
falls through without either reason. This baseline covers #3023's six signals;
it does not cover the other classifier files in #2691.

```expected-classes
probe:src_lib.rs:predicate:074e14d6 -> exposed
probe:src_lib.rs:return_value:014a6a43 -> exposed
probe:src_lib.rs:predicate:5489cf7e -> exposed
probe:src_lib.rs:return_value:1a5d4a37 -> exposed
probe:src_lib.rs:predicate:34247caa -> exposed
probe:src_lib.rs:return_value:15dd607d -> exposed
probe:src_lib.rs:predicate:a50d1313 -> exposed
probe:src_lib.rs:return_value:195474b0 -> exposed
probe:src_lib.rs:predicate:2193c650 -> exposed
probe:src_lib.rs:return_value:234fdd01 -> exposed
probe:src_lib.rs:predicate:d9c321f0 -> exposed
probe:src_lib.rs:return_value:3551dca4 -> exposed
probe:src_lib.rs:predicate:596eda33 -> exposed
probe:src_lib.rs:return_value:b2b99a3a -> exposed
```

## Must Not

- Infer runtime mutation outcomes or exact boundary coverage from a disclosure.
- Attribute an opaque signal from an unrelated test or expression to a finding.
- Treat the plain case as opaque solely because another case contains a signal.
