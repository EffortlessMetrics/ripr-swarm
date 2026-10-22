<!-- section: Fixed -->
- `ripr context` packets (version 1.1) now state the finding's exposure
  `classification`, `severity`, and `confidence` — the same fields
  `ripr check --json` findings carry — so agents holding the packet alone can
  name the verdict instead of deriving it from the underdetermined stage
  states (#8743).
