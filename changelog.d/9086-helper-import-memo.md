<!-- section: Changed -->
- `ripr check` computes each helper-uniqueness import answer once per run
  instead of re-parsing the test file for every probe, helper chain and test.
  A two-file petgraph diff that took 53.8 s now takes 4.4 s and the full
  8-file diff that ran past 400 s takes 8.1 s (release build, 4 cores); the
  output is byte-identical on the pair diff (#9086).
