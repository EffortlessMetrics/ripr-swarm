<!-- section: Internal / CI -->
- The repository docs gate reads committed document-role bytes on Windows: its
  `git archive` no longer applies `core.autocrlf` or a native CRLF `core.eol`,
  which made every role input mismatch its blob and the gate widen on every
  Windows checkout. Three native-Windows test fixtures now compare JSON-escaped
  and shell-quoted root spellings and name their temp trees by sequence (#16105).
