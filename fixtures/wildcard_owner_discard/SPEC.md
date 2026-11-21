# Fixture: wildcard_owner_discard

Spec: RIPR-SPEC-0094

Owner: analysis-fixtures

Issue: #6973

## Given

The owner result is discarded; an unrelated Some(_) assertion cannot observe tax.
The rate changes from nine to eight percent. The negative test passes both
versions; the named-binding test rejects the nine-percent version.

## When

`cargo xtask fixtures wildcard_owner_discard` runs the public diff producer.

## Then

Exactly one return-value finding is `weakly_exposed`, with the extracted
`observes_tax` test and its assertion retained. RIPR-SPEC-0108 independently
pins the class and nonempty finding count.

## Must Not

Grant owner identity to `_`, drop the finding or related assertion, or
confuse a leading underscore in a real binding with a wildcard.

```expected-classes
probe:src_lib.rs:return_value:40841afb -> weakly_exposed
```
