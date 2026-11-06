# Fixture: typescript_push_escaped_nondiscriminator

Spec: RIPR-SPEC-0243

Additional contract: RIPR-SPEC-0108; #12949.

## Given

The input string contains a literal backslash and `n`, while the expected
string contains a newline. Neither the original nor the uppercase value
matches that expected string.

## When

Analyze the committed change in `diff.patch` with the TypeScript adapter.

## Then

The one side-effect finding is `weakly_exposed`; different spellings and
escape decoding must not promote this assertion to changed-value evidence.

## Must Not

- Compare decoded input against the raw spelling of the expected literal.
- Treat static exposure as a runtime execution result.
