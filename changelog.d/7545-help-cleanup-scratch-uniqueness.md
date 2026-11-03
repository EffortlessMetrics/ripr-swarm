<!-- section: Internal / CI -->
- Help-proof scratch roots keep a process-wide sequence in the exclusive
  directory name so concurrent same-tick allocations, including nested
  process-group cleanup captures, do not fail with EEXIST (#7545).
