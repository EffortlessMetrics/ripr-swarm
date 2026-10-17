<!-- section: Added -->
- `cargo xtask work synthesis check` validates committed root-synthesis
  rows against the compiled work-portfolio basis: every synthesis binds
  the repository, portfolio-snapshot identity, basis head and the
  selected-work identity it reports on, reusing the RIPR-SPEC-0244
  selection law engine; the reported transition must match the selected
  action, every uncovered or contradicted acceptance row must be named in
  the contradictions list so a synthesis can never hide a gap, a reported
  campaign ref outside the bound selection's campaign refs is a silent
  switch that fails, the schema has no field that could create another
  selected issue, and legacy goal/work-item refs resolve read-only with no
  authority (#1706).
