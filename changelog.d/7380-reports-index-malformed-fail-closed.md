<!-- section: Fixed -->
- `ripr reports index` no longer counts a malformed JSON artifact as
  available. Availability for JSON-kind artifacts (agent receipt, SARIF,
  badge) now requires the bytes to parse as a JSON object; a garbage or
  non-object file renders an explicit `malformed` entry with its
  regeneration command, is excluded from the available count, emits a
  `malformed_artifact` warning, and keeps the packet status off `pass`
  instead of silently flipping it to pass (#7380).
