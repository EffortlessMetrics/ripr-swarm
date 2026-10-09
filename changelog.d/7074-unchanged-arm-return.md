<!-- section: Fixed -->
- A changed match-arm head (`"warning" | "warn" =>` from `"warn" =>`) no
  longer adds a `return_value` finding for the arm's unchanged result
  (`Ok(Level::Warn)`), which asked for a test of a value the edit never
  touched. The same applies to an unchanged `return` after a changed `if`
  condition. A return value the edit changed, or the only finding on a
  line, is kept (#7074).
