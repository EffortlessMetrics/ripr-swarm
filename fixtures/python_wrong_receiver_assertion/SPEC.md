# Fixture: python_wrong_receiver_assertion

Spec: RIPR-SPEC-0028

## Given

`Cart.total` changes from `amount * 2` to `amount * 3`. The only test calls `Cart().total(10)` and discards the result, then asserts `Other().total(10) == 30` on an unrelated class that always returns 30 (#8247).

## When

`cargo xtask fixtures python_wrong_receiver_assertion`

## Then

The finding is `weakly_exposed`: the method name matches, but the asserted `.total(` runs on a receiver that is not bound to the owner class.

## Must Not

Credit `exposed` because the same test body elsewhere binds a receiver to the owner class.
