<!-- section: Fixed -->
- `ripr check` no longer re-reads a long function once per related assertion
  of every changed line when it evaluates an assertion's expected value
  against the function: the function text is parsed and tokenized once per
  run, which takes about a quarter off the analysis time of a function with a
  thousand changed comparisons. Findings are unchanged (#11738).
