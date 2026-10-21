<!-- section: Changed -->
- Probe building maps each added line to the start of its contiguous run
  once per file. Looking up that start is then logarithmic, instead of
  scanning every added line at every step of the run. Verdicts are
  unchanged (#7524).
