<!-- section: Changed -->
- `ripr check` builds the shared candidate-index memo entries outside their
  locks, so parallel classification workers no longer queue behind one long
  scan. On a 359-probe diff the classify pass takes 33 s at 4 threads against
  50 s before and 115 s serial (release build, 4 cores); the output is
  byte-identical to the previous build on that diff (#8797, #8756).
