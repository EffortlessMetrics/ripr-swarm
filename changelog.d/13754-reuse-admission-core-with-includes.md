<!-- section: Changed -->
- Diff analysis no longer rebuilds the main index from scratch just because
  an admitted dependent file contains `include!`. The admission-core index is
  extended in place unless the added files change an already-indexed file's
  include parent or include-target status, add the first unresolved
  `include!`, exceed the include edge limit, or move an added file's function
  identities; in those cases it rebuilds from scratch as before (#13754).
