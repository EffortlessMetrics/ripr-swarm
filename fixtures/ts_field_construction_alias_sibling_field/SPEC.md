# Fixture: ts_field_construction_alias_sibling_field

Spec: RIPR-SPEC-0243

## Given

`buildConfig()` returns the object literal `{ retries, verbose }`, and the edit moves
`retries: 3 -> 5`. The only related test binds the result to a local alias
(`const cfg = buildConfig()`) and asserts a SIBLING field, `cfg.verbose`.

The fixture workspace enables the TypeScript preview adapter via `ripr.toml`.

## When

```bash
ripr check \
  --root fixtures/ts_field_construction_alias_sibling_field/input \
  --diff fixtures/ts_field_construction_alias_sibling_field/diff.patch \
  --mode fast
```

## Then

The changed field reports `weakly_exposed`, with `propagation_unknown` in the missing
evidence: a test that reads `verbose` through an alias cannot observe an edit to
`retries`, so the finding is partly noticed at most. This is the alias form of
#14814 (`fixtures/ts_field_construction_sibling_field`), matching the Rust precedent
`fixtures/one_hop_field_projection_sibling` (RIPR-SPEC-0159).

## Must Not

- Read `exposed` because the alias local was initialized from the owner call.
- Substitute a sibling field for the changed one.
- Lower a test that reads the CHANGED field by name through the same alias
  (`cfg.retries`) or the whole object; both pin the changed field and keep
  their verdict.

The expected class is pinned per finding (`expected/check.json`, checked by
`check-fixture-contracts`):

```expected-classes
probe:src_config.ts:typescript_preview:0bfbca8c -> weakly_exposed
```

Re-bless this fixture on Linux when the classifier wording or the digest changes:
`cargo xtask goldens bless ts_field_construction_alias_sibling_field --reason "RIPR-SPEC-0243:
<reviewed change>"`, which records the reason in this fixture's `expected/CHANGELOG.md`.
