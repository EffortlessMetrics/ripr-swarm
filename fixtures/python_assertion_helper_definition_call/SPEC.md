# Fixture: python_assertion_helper_definition_call

Spec: RIPR-SPEC-0238

## Given

A test default calls the owner on a module object, then a same-file helper checks the initial field of a different, local object with the same name.

## When

The expanded counter update changes to `+=`.

## Then

The finding remains `weakly_exposed`. The selected test body passes both the original owner and a deleted-write mutant.

## Must Not

Borrow a definition-time call as execution on a receiver constructed later. The RIPR-SPEC-0108 honesty corpus independently checks the rendered class.
