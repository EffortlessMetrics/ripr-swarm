# Fixture: parser_formatted_input_provenance

Spec: RIPR-SPEC-0240

## Given

The relevant UB-review #1419 `parse_note` body and direct test retain
formatted reason/input construction. Another test observes a different parser
condition; an unrelated same-file test observes plan and gate values.

## When

The unresolved-target reason predicate changes and the public analyzer reads
the fixture. Native runtime tests separately execute the parser and a compiling
validation bypass.

## Then

The actual parser callers retain their own identities. Unsupported formatted
input flow may remain unresolved. The unrelated plan/gate test cannot provide
this parser invocation's discriminator.

## Must Not

- Require static exposure merely because a runtime bypass fails.
- Treat another parser condition or a same-file assertion as activation proof.
- Claim a reduced fixture resolves the released full-consumer finding.
