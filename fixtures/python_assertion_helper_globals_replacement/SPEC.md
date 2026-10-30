# Fixture: python_assertion_helper_globals_replacement

Spec: RIPR-SPEC-0238

## Given

An imported field assertion helper is replaced in its defining module through globals replacement. Its executed replacement does not compare the changed field.

## When

The production counter update changes from expanded addition to `+=`.

## Then

One `Stats.record_write` finding remains `weakly_exposed`.

## Must Not

Credit the replaced helper body. Deleting the changed write leaves this test satisfied. RIPR-SPEC-0108 independently checks the class and nonempty count.
