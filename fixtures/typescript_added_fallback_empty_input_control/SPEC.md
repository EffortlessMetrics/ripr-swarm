# Fixture: typescript_added_fallback_empty_input_control

Spec: RIPR-SPEC-0243

Corpus case: `ts_added_fallback_empty_input_keeps_credit` in
`fixtures/evidence-promotion-honesty-corpus/corpus.json` (issue #15285 case 4).

## Given

`label` (src/label.ts) changes `return name;` to `return name || 'anon';`.
The only test asserts `expect(label('')).toBe('anon')`.

The fixture workspace enables the TypeScript preview adapter via `ripr.toml`.

## When

```bash
ripr check \
  --root fixtures/typescript_added_fallback_empty_input_control/input \
  --diff fixtures/typescript_added_fallback_empty_input_control/diff.patch \
  --mode fast
```

## Then

The finding reads `exposed`. The empty string is falsy, so the call supplies
the one input class on which the added fallback changes the result, and the
exact assertion distinguishes the two versions. This is the positive control
for `fixtures/typescript_added_fallback_populated_input` and
`fixtures/typescript_added_fallback_member_write`.

## Must Not

- Withhold credit from a call that is seen to supply a falsy argument for an
  added `||` fallback.
- Run any TypeScript runtime; static preview evidence only.

The expected class is pinned per finding (`expected/check.json`, checked by
`check-fixture-contracts`):

```expected-classes
probe:src_label.ts:typescript_preview:f1a70d02 -> exposed
```

Re-bless this fixture on Linux when the classifier wording or the digest changes:
`cargo xtask goldens bless typescript_added_fallback_empty_input_control --reason "RIPR-SPEC-0243:
<reviewed change>"`, which records the reason in this fixture's `expected/CHANGELOG.md`.
