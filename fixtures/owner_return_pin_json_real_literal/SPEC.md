# Fixture: owner_return_pin_json_real_literal

Spec: RIPR-SPEC-0197

## Given

The complete `Ok(())` callback return changes. The `real_literal` test constructs
JSON data with the canonical serde_json 1.0.154 macro or an adversarial control.
The real literal test preserves the didSave #17745 success body.

## When

The analyzer reads the exact diff and test source under locked registry identity.
The matched runtime test separately executes the correct and wrong callback.

## Then

The current `Ok(())` return receives an admitted equality oracle.
The positive fails on the wrong callback.
Every runtime subject compiles and executes one unignored test.

## Must Not

- Trust a macro by its name or lockfile entry alone.
- Credit source replacement, unresolved interpolation, or deferred assertions.
- Treat another return path or a removed finding as cleared.
