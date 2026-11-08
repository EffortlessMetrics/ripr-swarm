# Fixture: match_arm_proximity_other_function_not_credited

Spec: RIPR-SPEC-0094

Owner: analysis-fixtures

Issue: #11552

## Given

The diff changes the `Reduction::Over(wait) =>` arm in `Limiter::consume`.
`consume_under_budget` calls `consume` with a budget the arm never sees and
asserts on state the arm does not write. `reduce_over_budget` never calls
`consume`; it shares the file, makes an unindexed call (`.parse().unwrap()`)
that ripr cannot rule out as reaching `consume`, and asserts
`reduce(tokens, 10) == Reduction::Over(..)`. That assertion pins `reduce`'s
result, which has no name path to `consume`, not the arm.
These assertions are intentional analyzed fixture input, governed by the
existing `fixtures/**` source-input policy.

## When

```bash
cargo xtask fixtures match_arm_proximity_other_function_not_credited
```

The public diff analysis examines the changed arm on `src/lib.rs:29`.

## Then

The arm stays `weakly_exposed` with `observation_unverified`. The same-file
test cannot confirm the arm: its variant assertion reads `reduce`, an indexed
function that cannot run `consume` by any call or macro hop ripr can see, so
the unindexed `.unwrap()` is not enough to keep the confirmation #6297
withholds from a test that provably cannot reach the owner.

The same line also yields an `infection_unknown` predicate finding
(`wait.as_millis() < 10`), which this fixture does not govern.

Before #11552 the unindexed call kept the confirmation and the arm read
`exposed`, credited to an assertion on a different function (firecracker
`304718526`, `RateLimiter::consume` credited by `TokenBucket::reduce`'s
`OverConsumption` assertion).

## Must Not

- Confirm a match arm from a file-proximity test whose variant assertion reads
  an indexed function with no path to the owner.
- Withhold the confirmation when the assertion calls the owner, a function
  that reaches it, or a macro-hop caller of it.
- Claim runtime adequacy.
