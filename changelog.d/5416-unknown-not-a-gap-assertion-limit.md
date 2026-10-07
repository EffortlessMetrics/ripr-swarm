<!-- section: Fixed -->
- `ripr check` no longer reports a Rust `reachable_unrevealed` gap when every
  refused related `assert_eq!` was refused for a limit of ripr's own reading
  (an unparsed or unplaced file, an unidentified test, a feature `cfg`, a
  binding that only may rebind the macro) (RIPR-SPEC-0240). The finding is
  `static_unknown` with `static_limit_kind`
  `rust_assertion_context_unresolved`, stop reason `gap_evidence_unresolved`,
  no repair route, and a next step naming the tests to check. Refusals for an
  `if` branch, an uncalled closure, an opaque macro, a gated module or a real
  rebinding stay gaps. On the labeled verdict corpus false gaps on fully
  caught upstream edits fall from 7/20 to 6/20 with no new false silent or false
  exposed verdicts (#5416).
