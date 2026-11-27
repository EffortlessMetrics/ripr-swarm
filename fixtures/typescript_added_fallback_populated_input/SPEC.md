# Fixture: typescript_added_fallback_populated_input

Spec: RIPR-SPEC-0243

Corpus case: `ts_added_fallback_populated_input_passes_both_versions` in
`fixtures/evidence-promotion-honesty-corpus/corpus.json` (issue #15285 case 4).

## Given

The issue #15285 case 4 shape. `parse` (src/parse.ts) changes
`out[kv[0]] = kv[1];` to `out[kv[0]] = kv[1] || '';`. The only test asserts
an exact value on `parse('a=1&b=2')`, where every pair carries a value.

The fixture workspace enables the TypeScript preview adapter via `ripr.toml`.

## When

```bash
ripr check \
  --root fixtures/typescript_added_fallback_populated_input/input \
  --diff fixtures/typescript_added_fallback_populated_input/diff.patch \
  --mode fast
```

## Then

The finding reads `weakly_exposed`. The added `||` fallback changes the
result only when `kv[1]` is falsy. `kv` is a local derived from the
argument, so no related call is read as supplying that input, and the finding
names the input-class limitation ("only differs when `kv[1]` is ...").
`repair_packet_ready` stays `false`.

Before #15285 case 4 was fixed this read `exposed` on the strength of the
exact `toEqual` assertion, which passes on both versions.

## Must Not

- Read `exposed`: no test input leaves `kv[1]` empty, so the assertion
  cannot notice the fallback.
- Lower the empty-input control
  (`fixtures/typescript_added_fallback_empty_input_control`).
- Run any TypeScript runtime; static preview evidence only.

The expected class is pinned per finding (`expected/check.json`, checked by
`check-fixture-contracts`):

```expected-classes
probe:src_parse.ts:typescript_preview:eedafe47 -> weakly_exposed
```

Re-bless this fixture on Linux when the classifier wording or the digest changes:
`cargo xtask goldens bless typescript_added_fallback_populated_input --reason "RIPR-SPEC-0243:
<reviewed change>"`, which records the reason in this fixture's `expected/CHANGELOG.md`.
