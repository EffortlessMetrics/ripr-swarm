# Fixture: fixture_receiver_unread_data

Spec: RIPR-SPEC-0240

## Given

Two same-file helpers build literal numeric tuples. One vector contains a
cfg-excluded row; the other receiver has a raw-spelled by-value trait method
that wins dispatch over its inherent borrowed method. Method and helper names
are distinct across the files.

## When

Each inherent method swaps the tuple column it sums. Both original and changed
native programs still satisfy their exact assertion of seven. The excluded
tuple would separate the sums only if it existed; the competing trait method
returns seven without calling the changed inherent owner.

## Then

Both findings remain `static_unknown` with `rust_framework_oracle_unresolved`
and retain `fixture_opaque`. Attributed data and ambiguous owner dispatch
supply no literal fixture pin.

## Must Not

- Count a tuple removed by cfg as live receiver data.
- Treat raw spelling as a distinct method identity.
- Credit an assertion that dispatches to a different owner.
