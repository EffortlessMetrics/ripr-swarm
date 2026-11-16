# Fixture: ts_multi_value_field_change_sibling

Spec: RIPR-SPEC-0243 (Rule 24)

## Given

`buildConfig()` returns the object literal `{ retries, verbose, timeout }`. The only
related test asserts a third field, `buildConfig().timeout`, which the edit never
touches.

The fixture workspace enables the TypeScript preview adapter via `ripr.toml`.

## When

```bash
ripr check \
  --root fixtures/ts_multi_value_field_change_sibling/input \
  --diff fixtures/ts_multi_value_field_change_sibling/diff.patch \
  --mode fast
```

## Then

The edit moves TWO values on the returned line, `retries: 3 -> 5` and
`verbose: false -> true`. The change cannot be reduced to one field, so a read of the
untouched `timeout` cannot witness it: the finding reads `weakly_exposed` with the
observation unconfirmed, never `exposed`. This is the two-value shape that #14814's
single-field sibling guard does not reach (`changed_field` is `None` for it).

## Must Not

- Read `exposed` because the assertion's expression names the owner (`buildConfig()`).
- Substitute the untouched `timeout` for either changed field.
- Lower a test that reads EITHER changed field (`buildConfig().retries` or
  `buildConfig().verbose`) or the whole object; both pin a change and keep `exposed`.
  That control is the negative control: a fix that downgrades both is over-broad.
- Widen into the other unresolvable shapes (a key added/removed/renamed, shorthand or
  spread members, fewer than two fields); those keep their old reading and are filed
  separately.

The expected class is pinned per finding (`expected/check.json`, checked by
`check-fixture-contracts`):

```expected-classes
probe:src_config.ts:typescript_preview:10b44c74 -> weakly_exposed
```

This fixture ships no `expected/human.txt`, for the same reason as
`ts_field_construction_sibling_field`: the default human output prints the state token
`propagation_unknown` on its missing-check line, and the user-text gate refuses a banned
term on a new golden path (`check-user-text`, per-file delta against the merge base). The
verdict is pinned by `expected/check.json`; the full human surface stays in
`expected/human-full.txt`, where the glossary allowlists state tokens. Do not re-bless
this fixture: `cargo xtask goldens bless` always rewrites `human.txt` and would re-add it.
