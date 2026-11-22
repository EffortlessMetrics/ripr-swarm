<!-- section: Internal / CI -->
- `cargo xtask check-fixture-contracts` now checks
  `policy/fixture-denominator-floor.txt`, which names every optional golden
  surface and corpus case the fixture gates read. Deleting a golden or dropping
  a case used to remove its own comparison and leave `fixtures`, `goldens
  check`, `dogfood` and `check-fixture-contracts` green; it now fails with the
  subject's name until the line is removed in the same change. `cargo xtask
  fixtures` and `check-fixture-contracts` also refuse a run over no fixtures
  (#15925).
