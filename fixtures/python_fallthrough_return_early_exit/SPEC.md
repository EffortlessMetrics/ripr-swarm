# Fixture: python_fallthrough_return_early_exit

Spec: RIPR-SPEC-0260

## Given

`quote` raises its fallthrough multiplier from `1.05` to `1.10`. A simple
early exit `if total > 1000: return 0` precedes the changed return. The only
test that calls the owner asserts `quote(2000.0) == 0`, an exact strong
oracle whose argument provably satisfies the early-exit condition, so no
strong call executes the changed fallthrough return.

## When

`cargo xtask fixtures python_fallthrough_return_early_exit`

## Then

The finding is `weakly_exposed`, not `exposed`: every strong related call
takes the preceding early exit, so the changed fallthrough return has no
observed executing call. The missing discriminator names `not (total > 1000)`,
the input that would execute the changed return. This is the #9002
false-promotion correction: before the guarded-arm fallthrough rule, this
same shape read `exposed`.

## Must Not

Promote the finding to `exposed` while every strong call satisfies the
early-exit condition. The governed RIPR-SPEC-0108 honesty corpus
independently forbids promotion.
