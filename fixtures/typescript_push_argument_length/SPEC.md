# Fixture: typescript_push_argument_length

Spec: RIPR-SPEC-0243

Additional contract: RIPR-SPEC-0108; #12949.

## Given

A direct call appends either the original string or its uppercase form to a
fresh empty array. The related test asserts only the array length.

## When

Analyze the committed change in `diff.patch` with the TypeScript adapter.

## Then

The side-effect finding is `weakly_exposed`: the length assertion does not
observe the appended string. The honesty corpus independently refuses a
promotion to `exposed` even if the fixture output is regenerated.

## Must Not

- Promote a length-only assertion into credit for the appended value.
- Infer a runtime mutation result from static evidence.
