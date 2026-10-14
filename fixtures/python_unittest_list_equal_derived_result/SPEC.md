# Fixture: python_unittest_list_equal_derived_result

Spec: RIPR-SPEC-0028

## Given

`labels` changes its returned `Answer` flag from 0 to 1. The sole standard
unittest assertion compares `labels().fixed_list()` with literal `[1, 2]`.
That method returns the same list for both flags. Independent native execution
runs one test with zero failures/errors/skips for each version. Source provenance
is the CPython pin in `../python_unittest_list_equal/ORACLE.md`.

## When

`cargo xtask fixtures python_unittest_list_equal_derived_result`

## Then

One `static_unknown` finding retains an unknown custom-helper oracle. A derived
callee cannot establish comparison of the changed owner's whole returned value.

## Must Not

Report `exposed`, strong exact evidence, or discrimination yes. The independent
RIPR-SPEC-0108 honesty row forbids promotion and zero-finding clean output.
