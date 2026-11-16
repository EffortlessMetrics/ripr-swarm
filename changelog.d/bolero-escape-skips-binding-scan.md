<!-- section: Changed -->
- `ripr check` spends less time deciding whether a test's opaque macro is an
  admitted Bolero runner: the file-wide `use bolero::check` scan now runs only
  for a bare `check!` path, not once per test function for every opaque macro.
  On a warm ripr-swarm `check --base HEAD~1` the probe classification window
  dropped from about 15.4 s to 11.7 s and the whole run from about 39.6 s to
  37.1 s, with byte-identical JSON (#12947).
