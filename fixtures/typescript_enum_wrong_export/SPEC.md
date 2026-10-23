# Fixture: typescript_enum_wrong_export

Spec: RIPR-SPEC-0243

## Given

The same-module Other export is imported locally as Level. Its High value is 1, while the owner enum High is 2. The assertion on 10 passes both old and new versions because it reaches the unchanged arm.

## When

`cargo xtask fixtures typescript_enum_wrong_export` analyzes the changed true arm through the real CLI.

## Then

Exactly one finding at `src/level.ts:4` is `weakly_exposed`. The owner relation and strong exact-value oracle remain present. The independent honesty corpus pins that class and nonempty denominator.

## Must Not

- Infer enum identity from a local identifier or module name alone.
- Replace the assertion with a dead expected value.
- Change scalar conditional witnesses or TypeScript Preview support.
