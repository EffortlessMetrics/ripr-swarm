<!-- section: Internal / CI -->
- Coverage strips debug information from its test-profile binaries only during
  generation, with a mandatory native comparison of source coverage, counters
  and binary IDs. The control distinguishes DWARF from allocated debugger-loader
  metadata and reports actual section names, sizes and flags; full workspace
  storage savings require the retained native measurements (#4978).
