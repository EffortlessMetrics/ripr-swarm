# Fixture: typescript_jest_push_argument_length

Spec: RIPR-SPEC-0243

Additional contract: RIPR-SPEC-0108; #12949.

## Given

A direct test imports unaliased `test` and `expect` from `@jest/globals`
and passes a fresh empty array and the string `'hi'` to a single-statement
owner. The owner changes from appending the original string to appending its
uppercase form. The test asserts only the array length.

## When

Analyze the committed change in `diff.patch` with the TypeScript adapter.

## Then

The side-effect finding is `weakly_exposed`: both versions append one
element, so the length assertion does not distinguish the changed value.
The extracted equality keeps its raw `exact_value` / strong reading,
while the observation and discrimination stages remain weak.
The recommendation asks for an input changed by the edit and an assertion
on the appended element.

The honesty corpus independently rejects promotion to `exposed`, even if
the golden report is regenerated. The public CLI integration test
`discriminating_indexed_values_keep_credit_for_both_frameworks` supplies
the positive control: an exact index-zero pin that distinguishes `'hi'`
from `'HI'` retains credit under both supported frameworks.

## Must Not

- Treat array length or an unrelated equality as observing the changed string.
- Infer a runtime mutation result from static evidence.
