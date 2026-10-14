<!-- section: Internal / CI -->
- Python verdict corpus spec-example coverage: `fixtures/python-verdict-corpus`
  now has a `spec-coverage.toml` ledger, so `verdict-corpus check-all` gates
  its RIPR-SPEC-0238 citations on a floor (37 of 39 examples covered; 9 and
  10 stay visible as uncovered). Two new cases label example 34, and a
  floor failure now names the ledger of the corpus that failed (#7546).
