# Fixture: python_else_arm_no_exposed_credit

Spec: RIPR-SPEC-0260

## Given

`sign` changes the return of its `else` arm from `-1` to `-2`. The only test
that calls the owner asserts `sign(5) == 1`, an exact strong oracle whose
argument satisfies the `if x > 0` condition of the chain, so no strong call
enters the `else` arm that holds the changed line.

## When

`cargo xtask fixtures python_else_arm_no_exposed_credit`

## Then

The finding is `weakly_exposed`, not `exposed`: every strong related call takes
the earlier arm, so the changed `else` return has no observed executing call.
The missing discriminator names `not (x > 0)`, the input that would execute the
changed return. This is the #9184 false-promotion correction; the reproduced
mutant (`-1` to `-2`) passes the suite with only `sign(5)` tested.

## Must Not

Promote the finding to `exposed` while every strong call satisfies a chain
condition. The governed RIPR-SPEC-0108 honesty corpus independently forbids
promotion.
