<!-- section: Internal / CI -->
- The repository docs gate reads committed document-role bytes on Windows: its
  `git archive` no longer applies `core.autocrlf` or a native CRLF `core.eol`.
  In a repository without an `eol=lf` attribute (this repository pins one),
  every role input mismatched its blob and the gate widened. Three
  native-Windows test fixtures now compare JSON-escaped and separator-agnostic
  root spellings and name their temp trees by sequence (#16105).
