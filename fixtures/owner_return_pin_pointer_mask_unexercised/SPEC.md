# Fixture: owner_return_pin_pointer_mask_unexercised

Spec: RIPR-SPEC-0197

Honesty contract: RIPR-SPEC-0108.

## Given

A typed function pointer calls a masked scalar owner with 0x1234 and pins 0x34.

## When

The masked return is analyzed. The matched integration control compiles, lists
and executes one test each for the neutral 0xff and narrower 0x7f masks.

## Then

The changed return is not exposed: both runtime variants pass because the input
leaves bit 0x80 clear. The all-bit 0xff input is a separate positive control.

## Must Not

- Borrow direct-call mask coverage from the pointer target name in its binding.
- Promote a partial-bit input because its exact assertion calls the pointer.
