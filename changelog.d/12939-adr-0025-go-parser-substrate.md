<!-- section: Docs -->
- ADR 0025 (status: proposed) selects `tree-sitter-go` as the parser
  substrate for the planned Go adapter and allowlists the dependency. It is a
  decision record only: no Go analysis ships with it, and parser types are to
  stay inside `analysis::language::go` (#12939).
