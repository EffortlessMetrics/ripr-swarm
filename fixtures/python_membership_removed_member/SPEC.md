# Fixture: python_membership_removed_member

Spec: RIPR-SPEC-0238

## Given

The unchanged `authored-py-pytest-membership` verdict-corpus subject (the attrs
`to_bool` shape). Its parametrized `test_truthy` rows pass `true`, `t`, `yes`,
`on` and `1`, so no row passes `"y"`. The source case is
`fixtures/python-verdict-corpus/cases/py-membership-removed-member-unpassed.json`.

## When

The original corpus diff removes `"y"` from the literal truthy collection
`val in (True, "true", "t", "yes", "y", "on", "1", 1)`. The fixture input is the
after-edit tree; its retained tests are unchanged.

## Then

The changed owner has one `weakly_exposed` finding that names the missing case
`val == "y"`. The corpus runtime truth is `not_discriminated`: every test passes
on both versions, although `to_bool("y")` now raises (#15632).

## Must Not

Promote the changed owner to `exposed` because strong rows call `to_bool`, or
report a clean result. RIPR-SPEC-0108 independently enforces the finding count,
weak class, owner and `must_not_promote`, even if generated goldens are re-blessed.
