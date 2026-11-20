# Fixture: ts_multi_value_field_change_nonnull_negation

Spec: RIPR-SPEC-0243

Rule: 24 (a change that moves two field values on one line is not witnessed by a
read of a third, untouched field), through a postfix non-null assertion and a
prefix negation (#14928).

## Given

`buildConfig()` returns the object literal `{ retries, verbose, timeout, enabled }`.
The related tests read only untouched fields, through the two plain-chain shapes
the earlier operator check refused: `buildConfig()!.timeout` (postfix non-null)
and `!buildConfig().enabled` (prefix negation).

The fixture workspace enables the TypeScript preview adapter via `ripr.toml`.

## When

```bash
ripr check \
  --root fixtures/ts_multi_value_field_change_nonnull_negation/input \
  --diff fixtures/ts_multi_value_field_change_nonnull_negation/diff.patch \
  --mode fast
```

## Then

The edit moves TWO values on the returned line, `retries: 3 -> 5` and
`verbose: false -> true`. Neither test reads a changed field, so neither witnesses
the change: the finding reads `weakly_exposed`, never `exposed`. Before #14928 the
`!` in each base made it look compound, the untouched-field guard was skipped, and
the owner-call credit read `exposed`.

## Must Not

- Read `exposed` because the assertion's expression names the owner.
- Treat a postfix `!` or a prefix `!` as a compound operator that exempts the base
  from the untouched-field guard.

The expected class is pinned per finding (`expected/check.json`, checked by
`check-fixture-contracts`):

```expected-classes
probe:src_config.ts:typescript_preview:6e100598 -> weakly_exposed
```

This fixture ships no `expected/human.txt`, for the same reason as
`ts_multi_value_field_change_sibling`: the default human output prints a state token
that the user-text gate refuses on a new golden path. The verdict is pinned by
`expected/check.json`; the full human surface stays in `expected/human-full.txt`. Do not
re-bless this fixture without deleting the regenerated `human.txt`.
