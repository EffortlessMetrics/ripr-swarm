<!-- section: Fixed -->
- Pilot, agent brief and repair admission now honor unresolved Rust field
  snapshot evidence. Inspection retains the original class and stages; literal
  redacted-field gaps and exact field assertions remain available under their
  existing rules. Test names or other source mentions of the limitation code
  do not create a limitation. Each field keeps its own identity when
  initializers share a line or span several lines (#13278).
