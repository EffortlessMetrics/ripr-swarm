<!-- section: Fixed -->
- `ripr gate check` and `ripr gate evaluate` no longer report `pass` when a
  changed file could not be parsed (syntax error, nesting budget or non-UTF-8).
  The decision now carries a warning naming the file, so the status reads
  `advisory` and the Markdown says no gaps there is not evidence (#14082).
