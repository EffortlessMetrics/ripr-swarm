# Fixture: observation_verified_semicolon_effect

Spec: RIPR-SPEC-0094

## Given

`process_order` changes the payload of a standalone `notifier.send` statement
at line 18. Its semicolon discards the callee return; `true` follows at line 19.
The related test asserts the exact persisted payload.

## When

`cargo xtask fixtures observation_verified_semicolon_effect`

## Then

The aligned `@@ -16,5 +16,5 @@` hunk yields exactly one `call_deletion` and one
`side_effect` finding at line 18, both `exposed`. No `return_value` finding is
claimed. This is the standalone-call twin of `observation_verified_side_effect`.
A public analyzer regression pins this inventory, expression and coordinates.

Runtime controls separately establish that replacing the payload or deleting
the call breaks the payload assertion while a boolean-only assertion passes.
Both payloads return `true`; runtime effect discrimination supplies no evidence
that the returned boolean distinguishes this payload change.

## Must Not

- Credit a shifted tail-return hunk as standalone-call deletion coverage.
- Treat persisted-payload observation as observation of the returned boolean.
- Change production probe selection or assertion execution admission.
