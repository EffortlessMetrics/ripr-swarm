# Fixture: owner_return_pin_pointer_invisible_input

Spec: RIPR-SPEC-0197

Honesty contract: RIPR-SPEC-0108.

## Given

A typed immutable pointer asserts that capped(3) returns 3.

## When

The owner's cap changes from min(5) to min(6). Matched integration controls
compile, enumerate and run exactly one test on both versions, with bare,
qualified and reversed pointer calls and direct-call twins.

## Then

The changed return remains weakly exposed: both versions pass at input 3.
The existing bounded fold finds a separating input. At input 9 the changed
version passes an exact assertion of 6 and the baseline fails; those pointer
assertions remain credited. Equivalent rewrites retain the existing reading.

## Must Not

- Treat a checked value as proof that its input reaches the change.
- Weaken or remove the original assertion or one-test runtime inventory.
- Change the existing direct-call infection or owner-identity rules.
