# Fixture: rust_slice_counter_derived_result_constructor

Spec: RIPR-SPEC-0186. Honesty contract: RIPR-SPEC-0108. Issue: #5346.

## Given

A real local procedural derive emits a module-level `Ok` function that supplies
the firing error for count 17. The test imports standard Result constructors in
its own module and asserts the 16/17 calls on separate lines.

## When

The counter predicate changes from `count > 17` to `count >= 17`.

## Then

The predicate stays below `exposed`. Both native predicate variants pass with
the derive; without the producer the wrong predicate fails. Unknown expansion
cannot establish constructor identity, including aliased builtin derive names.
