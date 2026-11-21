# Fixture: javascript_push_argument_length

Spec: RIPR-SPEC-0243

Additional contract: RIPR-SPEC-0108; #12949 (JavaScript variant).

## Given

A direct call appends either the original string or its uppercase form to a
fresh empty array. The related test asserts only the array length. This is the
JavaScript (`.js`) counterpart of `typescript_push_argument_length`, through
the TypeScript-family adapter.

## When

Analyze the committed change in `diff.patch` with the TypeScript-family adapter.

## Then

The side-effect finding is `weakly_exposed` with `language = "javascript"`:
the length assertion does not observe the appended string. The honesty corpus
independently refuses a promotion to `exposed` even if the fixture output is
regenerated.

## Must Not

- Promote a length-only assertion into credit for the appended value.
- Relabel JavaScript findings as TypeScript.
- Infer a runtime mutation result from static evidence.
