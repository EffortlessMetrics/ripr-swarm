<!-- section: Fixed -->
- The DX scoreboard no longer treats an exit-zero check child whose stdout
  merely parses as JSON (`{}`, `null`, or another incomplete envelope) as a
  successful warm-check sample. Admission now requires a native check
  document, typed complete analysis, and the intended root/mode/base/head
  identity. Failed or incomplete runs keep their raw timings; a findings
  rendering cap is not treated as incomplete analysis, but trust cannot
  score contradictions over an uninspected prefix (#7259).
