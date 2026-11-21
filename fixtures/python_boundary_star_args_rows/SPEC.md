# Fixture: python_boundary_star_args_rows

Spec: RIPR-SPEC-0028

## Given

`clamp(value, floor=None)` changes `value < floor` to `value <= floor`. The
only related test is a pytest `parametrize` table whose rows are lists, and
the test calls the owner as `clamp(*args)`. No row puts `value` equal to
`floor`, so pytest passes both before and after the edit (#15287).

## When

`cargo xtask fixtures python_boundary_star_args_rows`

## Then

The single finding is `weakly_exposed` and names no missing discriminator:
the `*` unpack hides every input from the static reader, so no call is
established to reach the changed boundary.

## Must Not

Promote the predicate to `exposed` through the strong exact-value oracle, or
name `value == floor` as a repair target for inputs the reader did not see.
RIPR-SPEC-0108 enforces the class and `must_not_promote` independently of the
generated goldens.
