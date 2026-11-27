# Fixture: python_boundary_named_constant_rows

Spec: RIPR-SPEC-0028

## Given

`tier(points)` changes `points > 100` to `points >= 100`. The related test
is a pytest `parametrize` table whose rows name test-module constants
(`LOW = 20`, `HIGH = 250`), and it calls `tier(points)`. No row sits on
`100`, so pytest passes both before and after the edit (#15287).

## When

`cargo xtask fixtures python_boundary_named_constant_rows`

## Then

The single finding is `weakly_exposed` and names no missing discriminator: a
row that names a value is not read as a case, so the call's input is unread.

## Must Not

Promote the predicate to `exposed` because the inputs are spelled as names
instead of inline literals. RIPR-SPEC-0108 enforces the class and
`must_not_promote` independently of the generated goldens.
