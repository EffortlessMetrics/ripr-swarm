<!-- section: Changed -->
- The file-fact producer now records whether a file pulls documentation in
  from outside its text (`#![doc = include_str!(..)]`), using the parse it
  already holds. The unseen-reach scan reads that stored answer instead of
  re-parsing every candidate file on each run. Warm checks of two diffs ran
  about 0.9 s and 2 s faster, with byte-identical JSON (#13754).
