# Fixture: python_assertion_helper_rebound_class

Spec: RIPR-SPEC-0238

## Given

The imported production class is replaced with a class whose same-named method never writes the field.

## When

The expanded counter update changes to `+=`.

## Then

The finding remains `weakly_exposed`. The test passes both the original owner and a deleted-write mutant.

## Must Not

Borrow production method identity through a replaced class alias. The RIPR-SPEC-0108 honesty corpus independently checks the rendered class.
