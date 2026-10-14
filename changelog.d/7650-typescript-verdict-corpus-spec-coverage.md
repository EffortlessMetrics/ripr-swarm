<!-- section: Internal / CI -->
- TypeScript verdict corpus spec-example coverage:
  `fixtures/typescript-verdict-corpus` now has a `spec-coverage.toml` ledger,
  so `verdict-corpus check-all` gates its citations on a floor instead of
  scoring rows ungated. RIPR-SPEC-0243, its amendments RIPR-SPEC-0246 to
  RIPR-SPEC-0252, and RIPR-SPEC-0254 are in scope; 58 existing cases now cite
  the examples they label, covering 46 of 97 examples with the floor at 46 and
  none waived.
  RIPR-SPEC-0243 examples 30, 33 (#7099), 34 and 35 stay visible as uncovered.
  Every corpus but Perl now needs its ledger, so `check-all` fails rather than
  dropping the floor when one is deleted (#7650).
