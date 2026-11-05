<!-- section: Internal / CI -->
- Coverage emits bounded storage measurements before, during and after the
  existing full coverage command, preserving its profiles, tests and native
  result while distinguishing inspected allocation from partial evidence (#4978).
- Coverage strips debug information from its test-profile binaries only during
  generation, with a mandatory native comparison of source coverage, counters
  and binary IDs. The control distinguishes DWARF from allocated debugger-loader
  metadata and reports actual section names, sizes and flags; full workspace
  storage savings require the retained native measurements (#4978).
