<!-- section: Changed -->
- `ripr check` computes each helper-uniqueness import answer once per
  classification worker scope (each parallel worker memoizes independently)
  and once per indexed file per query, instead of re-parsing the test file for
  every probe, helper chain and test. A two-file petgraph diff that took
  62.4 s now takes 0.94 s and the full 8-file diff that ran past 400 s takes
  0.60 s (release build, 4 cores, one run each); the output is byte-identical
  on the pair diff (#9086).
