# Fixture: python_bool_owner_formatted_patch

Spec: RIPR-SPEC-0028
Spec: RIPR-SPEC-0108

## Given

A boolean owner changes its boundary comparison from `>` to `>=`.
The test replaces its imported `is_adult` binding through an executable formatted string
before calling it in a bare assertion.

## When

The changed comparison is analyzed.

## Then

One `static_unknown` finding, retaining the existing `mocked_module` limit.
The bare assertion remains smoke because it
can observe the replacement rather than the changed production callable.

## Must Not

Promote the assertion to an exact pin. Native Python still passes this test
when the production predicate returns false, so its apparent oracle does not
discriminate that wrong result. The honesty corpus rejects false promotion
independently of golden output.
