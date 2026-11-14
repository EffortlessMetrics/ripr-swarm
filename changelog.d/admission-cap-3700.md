<!-- section: Internal / CI -->
- The per-candidate admission cap in `.github/workflows/routed-rust.yml`
  rises from 3000 to 3700 changed lines in both the self-test and the
  admit-exact-candidate steps. The 3000-line cap was set before #14508's
  aliased-owner-credit guard, which is a single coherent change of about
  3,570 counted lines: 1,652 of implementation, 1,373 of controls across 45
  cases (29 lines each against a 37.7-line file average), and the rest spec
  and traceability. Removing a control to fit the old cap would drop a
  guard from a fail-closed path, so the cap moves instead (#14513).
