<!-- section: Fixed -->
- Resolving imported `macro_rules!` test generators skips the parser for files
  without a possible generator call. The conservative precheck retains Unicode
  boundaries, comments, raw names (including overlapping spellings in token
  trees), and qualified calls, while preserving grouped-import resolution (#10406).
