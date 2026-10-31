# Fixture: python_assertion_helper_defining_method

Spec: RIPR-SPEC-0238

## Given

The imported helper module replaces the observed method before the test calls it. The executed replacement leaves the observed field at zero.

## When

The production counter update changes from expanded addition to `+=`.

## Then

One `Stats.record_write` finding remains `weakly_exposed`.

## Must Not

Credit a method or constructor replaced by the helper defining module. Deleting the changed write leaves the test satisfied. RIPR-SPEC-0108 independently checks the class and nonempty count.
