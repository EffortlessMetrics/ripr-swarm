# Fixture: ts_changed_field_unchanged_child

Spec: RIPR-SPEC-0243

Rule: 25 (a read of an unchanged child of a changed field is not witnessed,
#14858).

## Given

`buildConfig()` returns `{ retries: { n, other }, verbose }`. The only related
test asserts `buildConfig().retries.other`, a child of the changed field that the
edit leaves at `1`.

The fixture workspace enables the TypeScript preview adapter via `ripr.toml`.

## When

```bash
ripr check \
  --root fixtures/ts_changed_field_unchanged_child/input \
  --diff fixtures/ts_changed_field_unchanged_child/diff.patch \
  --mode fast
```

## Then

The edit moves `retries.n` from `3` to `5` and leaves `retries.other` at `1`. The
test reads `other`, a value the change never alters, so it cannot witness the
change: the finding reads `weakly_exposed`, never `exposed`. Before #14858 the
read named the changed field `retries` on its path and was credited `exposed`.

## Must Not

- Read `exposed` because the read's path passes through the changed field.
- Lower a read of the moved child (`buildConfig().retries.n`) or of the whole
  field; those pin the change and keep `exposed`.

The expected class is pinned per finding (`expected/check.json`, checked by
`check-fixture-contracts`):

EXPECTED_CLASSES_PLACEHOLDER

This fixture ships no `expected/human.txt`, for the same reason as
`ts_multi_value_field_change_sibling`: the default human output prints a state token
that the user-text gate refuses on a new golden path. The verdict is pinned by
`expected/check.json`; the full human surface stays in `expected/human-full.txt`. Do not
re-bless this fixture without deleting the regenerated `human.txt`.
