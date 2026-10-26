<!-- section: Fixed -->
- `ripr agent repair --help` now states that `--phase` defaults to `before` (start a new attempt) and that `after` and `verify` require an existing repair attempt (`after` also supports selection by `--seam-id`), matching the parser default (#10542).
