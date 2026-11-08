# Fixture: python_bool_owner_mock_inspection_override

Spec: RIPR-SPEC-0028
Spec: RIPR-SPEC-0108

## Given

A boolean owner changes its boundary comparison from `>` to `>=`.
The test changes the callable's default threshold through a constructor keyword replacing a mock inspection method
before calling it in a bare assertion.

## When

The changed comparison is analyzed.

## Then

One `weakly_exposed` finding. The apparent inspection/environment operation
has no authenticated receiver contract, so the bare assertion remains smoke.

The separate mock inspection retains its existing `mock_expectation` /
`medium` oracle; the bare boolean assertion contributes no exact pin.

## Must Not

Promote the assertion to an exact pin. Native Python passes this test even
with the wrong `>` boundary because the local callback changes the default
threshold. The honesty corpus independently rejects false promotion.
