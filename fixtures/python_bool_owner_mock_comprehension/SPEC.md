# Fixture: python_bool_owner_mock_comprehension

Spec: RIPR-SPEC-0028
Spec: RIPR-SPEC-0108

## Given

A boolean owner changes its boundary comparison from `>` to `>=`.
The test changes the callable's default threshold through a comprehension target shadowing an earlier genuine mock receiver
before the bare assertion.

## When

The changed comparison is analyzed.

## Then

One `weakly_exposed` finding. Receiver spelling and method name do not
establish an inert operation, so the bare assertion remains smoke.

## Must Not

Promote the assertion to an exact pin. Native Python passes this test even
with the wrong `>` boundary because the operation changes the default
threshold. The honesty corpus independently rejects false promotion.
