<!-- section: Internal / CI -->
- Candidate admission treats a verdict corpus's producer fact packets (`fixtures/<language>-verdict-corpus/packets/**`, data only and sha256-pinned by the case rows) as declared generated files, so a corpus PR is sized on its authored cases, subjects and diffs; the separate generated-line cap still applies (#9447).
