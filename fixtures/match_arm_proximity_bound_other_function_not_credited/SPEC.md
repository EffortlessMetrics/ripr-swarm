# Fixture: match_arm_proximity_bound_other_function_not_credited

Spec: RIPR-SPEC-0094

Owner: analysis-fixtures

Issue: #12056

## Given

The diff changes the `Reduction::Over(wait) =>` arm in `Limiter::consume`.
`consume_under_budget` calls `consume` with a budget the arm never sees and
asserts on state the arm does not write. `reduce_over_budget` never calls
`consume`; it shares the file, makes an unindexed call (`.parse().unwrap()`)
that ripr cannot rule out as reaching `consume`, binds
`let reduced = reduce(tokens, 10);` on one line and asserts
`assert_eq!(reduced, Reduction::Over(..))` on the next. The assertion text
holds no call, but the local it reads is `reduce`'s result, which has no name
path to `consume`.
These assertions are intentional analyzed fixture input, governed by the
existing `fixtures/**` source-input policy.

## When

```bash
cargo xtask fixtures match_arm_proximity_bound_other_function_not_credited
```

The public diff analysis examines the changed arm on `src/lib.rs:29`.

## Then

The arm stays `weakly_exposed` with `observation_unverified`, the same verdict
as `match_arm_proximity_other_function_not_credited`, where the call sits in
the assertion. The same-file test cannot confirm the arm through a local bound
to a function that cannot run `consume`.

Before #12056 the assertion text carried no call, so the unindexed `.unwrap()`
kept the confirmation and the arm read `exposed`.

## Must Not

- Confirm a match arm from a file-proximity test whose variant assertion reads a
  local bound to an indexed function with no path to the owner.
- Resolve a `mut` local, a local with several bindings or one the scan cannot
  read: those leave the assertion unattributed.
- Claim runtime adequacy.
