# Fixture: python_assertion_helper_inactive_call

Spec: RIPR-SPEC-0238

## Given

An owner call in an expression that never executes, followed by an exact same-file helper check of the initial field.

## When

The expanded counter update changes to `+=`.

## Then

The finding remains `weakly_exposed`. The selected test body passes both the original owner and a deleted-write mutant.

## Must Not

Treat a syntactic call hidden behind `False and` as execution of the changed effect. The RIPR-SPEC-0108 honesty corpus independently checks the rendered class.
