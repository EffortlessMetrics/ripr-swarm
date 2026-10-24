# Fixture: owner_return_pin_pointer_mask_rebound

Spec: RIPR-SPEC-0197

Honesty contract: RIPR-SPEC-0108.

## Given

A typed function pointer passes the all-bit input 255 to a masked scalar owner.
The owner shadows that parameter with 52 before applying the mask.

## When

The changed masked return is analyzed. Matched integration controls compile,
enumerate and execute one test for both the neutral 0xff and narrower 0x7f masks.

## Then

The changed return remains weakly exposed. Both native variants pass because
the actual masked operand is 52, whose bit 0x80 is clear. A second integration
control transforms the input with `let n = n & 0x34` and has the same blind spot.

## Must Not

- Treat an all-bit argument as coverage of an operand the owner overwrites.
- Change the original assertion or weaken its native execution inventory.
- Change the existing direct-call mask guard's rebinding behavior.
