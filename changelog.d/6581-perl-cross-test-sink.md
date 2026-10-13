<!-- section: Fixed -->
- A Perl change no longer reads `exposed` because another test's oracle
  observes the changed sink. Sink alignment now requires the oracle to belong
  to the relation's test, the binding classification already used
  (RIPR-SPEC-0235 rule 5 and example 28, #6581).
