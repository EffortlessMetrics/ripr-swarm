<!-- section: Fixed -->
- CI admission: the source-review disposition verifier no longer reads a
  sentence-position negated blocker phrase ("No source blocker found.",
  "no source blockers remain") as a blocker, so a truthful current-head
  attestation is not vetoed by its own resolved finding; a later blocker
  sentence, a same-line continuation, and a refuted negation still refuse
  (#15980).
