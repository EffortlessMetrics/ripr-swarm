# Fixture: typescript_added_fallback_parameter_default_write

Spec: RIPR-SPEC-0243

Corpus case: `ts_added_fallback_parameter_default_overrides_the_argument` in
`fixtures/evidence-promotion-honesty-corpus/corpus.json` (issue #15285 case 4).

## Given

`label` (src/label.ts) declares `other = (name = 'Bob')` after `name`, and
changes `return name;` to `return name || 'anon';`. The only test calls
`label(null)`, omitting `other`, and asserts `'Bob'`.

The fixture workspace enables the TypeScript preview adapter via `ripr.toml`.

## When

```bash
ripr check \
  --root fixtures/typescript_added_fallback_parameter_default_write/input \
  --diff fixtures/typescript_added_fallback_parameter_default_write/diff.patch \
  --mode fast
```

## Then

The finding reads `weakly_exposed`. A `null` argument would be a witness for
the added `||` fallback, but `ts_operand_binding_unstable` sees the later
parameter default write `name` before the body runs, so the argument is not
credited. Both versions return `'Bob'`. The finding says that `label` writes
or rebinds `name` before the fallback reads it and suggests no input;
`repair_packet_ready` stays `false`.

Before this guard the `null` argument was credited and the finding read
`exposed`.

## Must Not

- Credit an argument that another parameter's default overwrites before the
  fallback reads it.
- Lower the parameter's-own-default positive in
  `crates/ripr/tests/typescript_added_fallback.rs`.
- Run any TypeScript runtime; static preview evidence only.

The expected class is pinned per finding (`expected/check.json`, checked by
`check-fixture-contracts`):

```expected-classes
probe:src_label.ts:typescript_preview:f1a70d02 -> weakly_exposed
```

Re-bless this fixture on Linux when the classifier wording or the digest changes:
`cargo xtask goldens bless typescript_added_fallback_parameter_default_write --reason "RIPR-SPEC-0243:
<reviewed change>"`, which records the reason in this fixture's `expected/CHANGELOG.md`.
