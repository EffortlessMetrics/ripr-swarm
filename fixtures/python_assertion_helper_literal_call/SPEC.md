# Fixture: python_assertion_helper_literal_call

Spec: RIPR-SPEC-0238

## Given

A multiline literal names an owner call that never executes, followed by a helper check of the initial field.

## When

The expanded counter update changes to `+=`.

## Then

The finding remains `weakly_exposed`. Deleting the counter update still passes the test.

## Must Not

Credit text inside a multiline literal as execution of the changed owner. The RIPR-SPEC-0108 honesty corpus independently checks the rendered class.
