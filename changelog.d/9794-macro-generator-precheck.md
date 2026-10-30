<!-- section: Fixed -->
- Resolving imported `macro_rules!` test generators skips the parser even when
  a generator name occurs only in identifier lookalikes. The precheck retains Unicode
  boundaries, comments, raw names (including overlapping spellings in token
  trees), and qualified calls, while preserving grouped-import resolution (#10406).
