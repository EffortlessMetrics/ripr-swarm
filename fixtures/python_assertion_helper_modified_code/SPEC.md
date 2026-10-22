# Fixture: python_assertion_helper_modified_code

Spec: RIPR-SPEC-0238

## Given

A helper comparison whose modified code cannot establish observation of the changed field.

## When

The expanded counter update changes to `+=`.

## Then

The finding remains `weakly_exposed`. Deleting the counter update still passes the selected test body.

## Must Not

Promote through source-line coincidence, a modified function body, or an owner-name token.
The RIPR-SPEC-0108 honesty corpus independently checks the rendered class.
