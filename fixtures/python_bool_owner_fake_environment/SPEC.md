# Fixture: python_bool_owner_fake_environment

Spec: RIPR-SPEC-0028
Spec: RIPR-SPEC-0108

## Given

A boolean owner changes its boundary comparison from `>` to `>=`.
The test changes the callable's default threshold through an ordinary helper parameter receiving a local object
before calling it in a bare assertion.

## When

The changed comparison is analyzed.

## Then

One `weakly_exposed` finding. The apparent inspection/environment operation
has no authenticated receiver contract, so the bare assertion remains smoke.

## Must Not

Promote the assertion to an exact pin. Native Python passes this test even
with the wrong `>` boundary because the local callback changes the default
threshold. The honesty corpus independently rejects false promotion.
