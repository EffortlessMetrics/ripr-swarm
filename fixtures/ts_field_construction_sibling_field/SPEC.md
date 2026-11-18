# Fixture: ts_field_construction_sibling_field

Spec: RIPR-SPEC-0243

## Given

`buildConfig()` returns the object literal `{ retries, verbose }`, and the edit moves
`retries: 3 -> 5`. The only related test asserts a SIBLING field, `buildConfig().verbose`,
which it reads off the same returned object.

The fixture workspace enables the TypeScript preview adapter via `ripr.toml`.

## When

```bash
ripr check \
  --root fixtures/ts_field_construction_sibling_field/input \
  --diff fixtures/ts_field_construction_sibling_field/diff.patch \
  --mode fast
```

## Then

The changed field reports `weakly_exposed`, with `propagation_unknown` in the missing
evidence: a test that reads `verbose` cannot observe an edit to `retries`, so the finding
is partly noticed at most. This matches the Rust precedent
`fixtures/one_hop_field_projection_sibling` (RIPR-SPEC-0159) and the Python sibling-field
rule.

## Must Not

- Read `exposed` because the assertion's expression names the owner (`buildConfig()`).
- Substitute a sibling field for the changed one.
- Lower a test that reads the CHANGED field by name (`buildConfig().retries`) or the whole
  object; both pin the changed field and keep their verdict.

The expected class is pinned per finding (`expected/check.json`, checked by
`check-fixture-contracts`):

```expected-classes
probe:src_config.ts:typescript_preview:0bfbca8c -> weakly_exposed
```

This fixture ships `expected/human.txt` (RIPR-SPEC-0122, #11619). The default human digest
drops a leading state-token label from the missing-check line, so that line opens with the
classifier wording - `changed value sinks to the owner return value or constructed object` -
and the `propagation unknown` phrase follows on the wrapped continuation instead of leading
it. That is why the user-text gate (`check-user-text`, per-file delta against the merge base)
no longer refuses a banned snake_case state token on this new golden path. The verdict is
pinned by `expected/check.json` and by the honesty corpus case
`ts_field_construction_sibling_field_read_does_not_pin_changed_field`; the full human surface
stays in `expected/human-full.txt`, where the glossary allowlists state tokens and the
missing-check line still leads with `propagation_unknown`.

Re-bless this fixture on Linux when the classifier wording or the digest changes:
`cargo xtask goldens bless ts_field_construction_sibling_field --reason "RIPR-SPEC-0243:
<reviewed change>"`, which records the reason in this fixture's `expected/CHANGELOG.md`.
