# Fixture: python_bound_receiver_assertion

Spec: RIPR-SPEC-0028

## Given

The same `Cart.total` change as `python_wrong_receiver_assertion`. The test asserts `cart.total(10) == 30` on a receiver bound to `Cart` beside an unrelated `other.total(10)` assertion.

## When

`cargo xtask fixtures python_bound_receiver_assertion`

## Then

The finding is `exposed`: at least one asserted `.total(` call runs on a receiver bound to the owner class.

## Must Not

Withhold credit from an assertion on the bound receiver because an unrelated receiver is asserted nearby. This control keeps the foreign-receiver gate from becoming a blanket veto.
