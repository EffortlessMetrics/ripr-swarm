# Fixture: typescript_reassigned_setup_hook

Spec: RIPR-SPEC-0243

## Given

A fake receiver has an exact-value assertion. The ambient `beforeEach` name is
reassigned to an inert function before receiving the callback that constructs
the changed owner class. The callback never runs, so the assertion observes
the fake receiver rather than `Cart.total`.

## When

`ripr check` analyzes `Cart.total` changing from `return 0` to `return 1`.

## Then

The finding stays `no_static_path`: no trusted receiver relation or strong
oracle is attributed to the changed owner. It must never read `exposed`.
The independent evidence-promotion-honesty corpus pins this requirement.
The enabled-hook control in the scope receiver tests must retain `exposed`.

Actual Mocha 12.0.3 controls pass with the inert hook before and after changing
the real owner return from 1 to 2; the enabled-hook control rejects that change.
Those runtime controls establish the fixture premise, not a RIPR runtime claim.

## Must Not

- Attribute the fake receiver's exact assertion to `Cart.total`.
- Credit construction inside a callback given to an inert hook.
- Remove the established enabled-hook positive control.
