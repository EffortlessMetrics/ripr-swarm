# Fixture: typescript_added_fallback_member_write

Spec: RIPR-SPEC-0243

Corpus case: `ts_added_fallback_member_write_overrides_the_argument` in
`fixtures/evidence-promotion-honesty-corpus/corpus.json` (issue #15285 case 4).

## Given

`title` (src/title.ts) writes `opts.name = 'Bob'` and then changes
`return opts.name;` to `return opts.name || 'untitled';`. The only test
calls `title({})`, whose `name` field is missing, and asserts `'Bob'`.

The fixture workspace enables the TypeScript preview adapter via `ripr.toml`.

## When

```bash
ripr check \
  --root fixtures/typescript_added_fallback_member_write/input \
  --diff fixtures/typescript_added_fallback_member_write/diff.patch \
  --mode fast
```

## Then

The finding reads `weakly_exposed`. The missing field would be a witness for
the fallback, but `ts_operand_binding_unstable` sees the member write to
`opts` before the read, so the argument is not credited. Both versions return
`'Bob'`. The finding names the input-class limitation, says that `title`
writes `opts.name` before the fallback reads it, and suggests no input;
`repair_packet_ready` stays `false`.

Before this guard the missing field was credited and the finding read
`exposed`.

## Must Not

- Credit the call's missing field when the owner overwrites it before the
  fallback reads it.
- Lower the no-write missing-field positive in
  `crates/ripr/tests/typescript_added_fallback.rs`.
- Run any TypeScript runtime; static preview evidence only.

The expected class is pinned per finding (`expected/check.json`, checked by
`check-fixture-contracts`):

```expected-classes
probe:src_title.ts:typescript_preview:a7749074 -> weakly_exposed
```

Re-bless this fixture on Linux when the classifier wording or the digest changes:
`cargo xtask goldens bless typescript_added_fallback_member_write --reason "RIPR-SPEC-0243:
<reviewed change>"`, which records the reason in this fixture's `expected/CHANGELOG.md`.
