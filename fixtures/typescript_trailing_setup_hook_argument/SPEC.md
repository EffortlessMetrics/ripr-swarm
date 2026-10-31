# Fixture: typescript_trailing_setup_hook_argument

Spec: RIPR-SPEC-0243

## Given

A Mocha suite replaces the ambient `beforeEach` in an argument following its
callback. The callback offers a receiver constructor through that inert hook.
The assertion reads a Decoy receiver.

## When

`ripr check` analyzes `Cart.total` changing from `return 1` to `return 2`.

## Then

The finding remains `no_static_path`. All suite arguments precede callback
invocation. Independent runtime controls pass both owner versions with the inert setup;
removing the write makes the changed owner fail. Direct imported Vitest hooks
remain discriminating despite an ambient write.

## Must Not

- Execute a suite callback before evaluating trailing call arguments.
- Revoke an earlier completed registration after a later global write.
- Withhold independent imported runner hooks because ambient globals change.
