# Fixture: typescript_untaken_arm_reachable_unrevealed

Spec: RIPR-SPEC-0027
Spec: RIPR-SPEC-0108

## Given

The changed line sits inside the `coupon === 'HALF'` arm of `price`. The only
related test calls `price(2)` with no coupon, so no test input ever enters the
changed arm, and its oracle is a weak `toBeGreaterThan` check (the untaken-arm
shape of #10704).

## When

`cargo xtask fixtures typescript_untaken_arm_reachable_unrevealed` analyzes
the changed arm through the real CLI.

## Then

One `reachable_unrevealed` finding with infection `no`
(`branch_not_entered`): the test reaches the owner, but the changed line never
runs under it, so nothing can reveal the edit.

## Must Not

Read the finding as `weakly_exposed` with an "assertion too weak" explanation,
promote it to `exposed`, or emit a delegatable repair packet for it. The
RIPR-SPEC-0108 honesty corpus independently forbids promotion.
