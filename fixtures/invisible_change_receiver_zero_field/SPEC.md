# Fixture: invisible_change_receiver_zero_field

Spec: RIPR-SPEC-0197

## Given

`Acc::double` returned `self.base * 2` and the diff changes the factor to `3`.
The only related test asserts `Acc { base: 0 }.double() == 0`.

## When

The exact diff is analyzed.

## Then

The return-value finding is not `exposed`: with `base` at `0` both versions
return `0`, so the asserted receiver cannot notice the change. The method is
read through the receiver field the record literal spells (#15272).

## Must Not

- Promote the change to `exposed` from an exact assertion on a receiver whose
  field value hides the changed factor.
