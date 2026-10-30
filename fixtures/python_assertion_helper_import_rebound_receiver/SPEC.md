# Fixture: python_assertion_helper_import_rebound_receiver

Spec: RIPR-SPEC-0238

## Given

A same-file helper whose import rebound receiver permits a deleted-write mutant to survive.

## When

The expanded counter update changes to `+=`.

## Then

The finding remains `weakly_exposed`, with one finding retained.

## Must Not

Credit a replaced receiver, modified effect or expected value copied from the effect. The RIPR-SPEC-0108 honesty corpus independently checks the JSON and human class.
