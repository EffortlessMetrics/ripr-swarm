<!-- section: Changed -->
- LSP: hovering a TypeScript, JavaScript or Perl preview finding whose
  repair packet is not ready now shows one line,
  "Repair instructions for an AI agent: not ready — <why>", instead of the
  packet contract's state, category, route, missing-field list, promotion
  criteria and a second "advisory only" line. Diagnostic `data`, check JSON
  and the context packet still carry those fields; the ready case is
  unchanged (#14353).
- LSP: preview-language diagnostics say "preview" once. The Problems panel row
  read `python preview evidence (syntax-first, advisory): Python preview: …`;
  it now reads `Python preview (advisory): …`, with product casing. The hover
  still names the syntax-first evidence boundary (#14353).
