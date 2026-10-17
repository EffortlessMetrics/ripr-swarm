# Fixture: python_cross_test_import_alias

Spec: RIPR-SPEC-0028

## Given

`fee` changes from `amount * 2` to `amount * 3`. The only test that reaches it
discards the result and asserts `quote(10) == 30`, where `quote` is
`src.fixed.fixed` and always returns 30. A second test that never calls the
owner imports `fee as quote` inside its own body. Native execution passes the
asserting test with either production return (#8247).

## When

`cargo xtask fixtures python_cross_test_import_alias`

## Then

The finding is not `exposed`: an import alias binds only in the test that
imports it, so the alias in the other test does not make the asserting test's
`quote` the owner.

## Must Not

Report `exposed` through an `alias` alignment borrowed from another test.
The governed RIPR-SPEC-0108 honesty corpus independently forbids promotion.
