<!-- section: Fixed -->
- Python verification guidance withholds unittest selectors for literal dots
  in directory or filename components, which could run a different file's
  same-named test; related-test evidence remains visible. Repair guidance keeps
  a later runnable test and describes that selected test's evidence (#7592).
