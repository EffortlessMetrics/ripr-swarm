<!-- section: Added -->
- TypeScript verdict corpus spec-example coverage:
  `fixtures/typescript-verdict-corpus` now has a `spec-coverage.toml` ledger,
  so `verdict-corpus check-all` gates its citations on a floor instead of
  scoring rows ungated. RIPR-SPEC-0243 and its amendments RIPR-SPEC-0246 to
  RIPR-SPEC-0252 are in scope; 57 existing cases now cite the examples they
  label, covering 48 of 92 examples with the floor at 48 and none waived.
  RIPR-SPEC-0243 example 33 stays visible as uncovered (#7099).
