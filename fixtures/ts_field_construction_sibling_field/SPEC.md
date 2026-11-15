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
