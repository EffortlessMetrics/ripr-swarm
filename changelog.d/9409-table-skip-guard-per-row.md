<!-- section: Fixed -->
- A table test that skips known-bad rows with `if SKIP.contains(&key) { continue; }`
  (or `if key == "literal" { continue; }`) is no longer read as skipping every
  row. When the module `const` resolves to plain literals and the loop variable
  is a literal cell of each row, ripr drops only the rows the guard names and
  credits the assertion on the rest. Anything it cannot decide keeps the old
  reading. A return value that an edit only respells (`21` to `0x15`) no longer
  gets a finding of its own (#9409).
