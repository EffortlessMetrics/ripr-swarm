# Fixture: wildcard_owner_named

Spec: RIPR-SPEC-0094

Owner: analysis-fixtures

Issue: #6973

## Given

The named _amount binding carries the owner result into an exact assertion.
The rate changes from nine to eight percent. The negative test passes both
versions; the named-binding test rejects the nine-percent version.

## When

`cargo xtask fixtures wildcard_owner_named` runs the public diff producer.

## Then

Exactly one return-value finding is `exposed`, with the extracted
`observes_tax` test and its assertion retained. RIPR-SPEC-0108 independently
pins the class and nonempty finding count.

## Must Not

Grant owner identity to `_`, drop the finding or related assertion, or
confuse a leading underscore in a real binding with a wildcard.

```expected-classes
probe:src_lib.rs:return_value:40841afb -> exposed
```
