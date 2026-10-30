# Fixture: typescript_enum_canonical_alias

Spec: RIPR-SPEC-0243

## Given

The canonical enum is imported under a different local name. The assertion on 10 passes the old version and fails the new version because Rank.High names the owner enum High value 2.

## When

`cargo xtask fixtures typescript_enum_canonical_alias` analyzes the changed true arm through the real CLI.

## Then

Exactly one finding at `src/level.ts:4` is `exposed`. The owner relation and strong exact-value oracle remain present. The independent honesty corpus pins that class and nonempty denominator.

## Must Not

- Infer enum identity from a local identifier or module name alone.
- Replace the assertion with a dead expected value.
- Change scalar conditional witnesses or TypeScript Preview support.
