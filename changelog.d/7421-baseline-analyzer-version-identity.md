<!-- section: Fixed -->
- `ripr baseline create` now persists the producing gate decision's
  `subject.analyzer_version` (issue #5474) into the `gate_baseline` ledger
  under an additive `subject` block, and warns loudly — on stdout and in the
  ledger — when the source report predates build identity. `ripr baseline
  diff` joins the two sides' builds: an unmatched baseline entry across a
  provable analyzer-build boundary is disclosed as a typed stale
  identity-discontinuity item (carrying both versions and a refresh route,
  plus a warning and a Markdown section) instead of silently counting as
  `resolved` — so an analyzer upgrade that changes identity derivation
  (#7250) can no longer read identity loss as a debt improvement. `ripr zero
  status` therefore withholds `RIPR 0: achieved` while such entries are
  outstanding; same-build and unversioned old-vs-old diffs keep the legacy
  `resolved` behavior (#7421).
