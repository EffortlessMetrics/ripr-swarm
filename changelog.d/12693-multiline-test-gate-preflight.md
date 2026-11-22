<!-- section: Fixed -->
- The repair-start dirty-premise preflight now recognises a multiline
  `#[cfg(\n    test\n)]` inline test module gate the same way as the one-line
  spelling, so a file carrying it is deferred to the exact cage policy instead
  of being named as a dirty production path. Unterminated or ambiguous
  attribute text also defers rather than refusing, keeping the preflight
  conservative (#11808).
