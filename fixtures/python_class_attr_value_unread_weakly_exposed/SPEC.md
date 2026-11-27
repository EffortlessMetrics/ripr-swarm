# Fixture: python_class_attr_value_unread_weakly_exposed

Spec: RIPR-SPEC-0238

## Given

`Clock` changes its class attribute `_PER_YEAR` from `12` to `13`. Only
`years` reads it. The only test asserts `Clock(48).days() == 2`; `days` reads
`_PER_DAY`, never `_PER_YEAR`, so the assertion passes with both values.

## When

`cargo xtask fixtures python_class_attr_value_unread_weakly_exposed`

## Then

The finding is `weakly_exposed`, not `exposed`, and its missing discriminator
names `_PER_YEAR` and the reader `years` (#16009). Building the class and
asserting on an unrelated method reaches the owner without observing the
value.

## Must Not

Promote the finding to `exposed` from a strong oracle on a method that does
not read the changed attribute.
