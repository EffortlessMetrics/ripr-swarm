# Fixture: python_caplog_alias_in_literal

Spec: RIPR-SPEC-0238

## Given

The test reads `msgs = caplog.text` after the owner call and asserts
`"msgs: 5" in msgs`. The alias name is also spelled inside the asserted
literal.

## When

The logged message loses its ` caplog.text: 5` suffix and keeps `msgs: 5`.

## Then

The asserted text is still logged after the edit, so the finding is a weak
gap and is never `exposed`.

## Must Not

Replace the alias name inside the quoted literal: reading it as
`caplog.text: 5` would credit an assertion that still passes on the kept
message (rule 22, #10151).
