# Fixture: owner_return_pin_json_nested_literal

Spec: RIPR-SPEC-0197

## Given

The complete `Ok(())` callback return changes. The `nested_literal` test constructs
JSON data with the canonical serde_json 1.0.151 macro or an adversarial control.
The real literal test preserves the didSave #17745 success body.

## When

The analyzer reads the exact diff and test source under locked registry identity.
The matched runtime test separately executes the correct and wrong callback.

## Then

The current `Ok(())` return receives an admitted equality oracle but remains
weakly exposed because the owner-call argument pin is missing.
The wrong callback fails at the equality.
Every runtime subject compiles and executes one unignored test.

## Must Not

- Trust a macro by its name or lockfile entry alone.
- Credit source replacement, unresolved interpolation, or deferred assertions.
- Treat another return path or a removed finding as cleared.
