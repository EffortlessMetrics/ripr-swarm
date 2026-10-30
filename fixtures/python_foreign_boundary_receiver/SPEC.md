# Fixture: python_foreign_boundary_receiver

Spec: RIPR-SPEC-0028

A test asserts `Eligible().check(5)` and `Other().check(3)` while the changed
predicate belongs to `Eligible.check`. Both assertions pass with either
predicate. A call on `Other` cannot witness the changed owner's boundary
(#11044); its receiver is not established as the owner.

The finding must not be promoted to `exposed` or name `x == 3` as missing based
on an unplaced receiver. A separate owner-bound boundary control retains credit.
