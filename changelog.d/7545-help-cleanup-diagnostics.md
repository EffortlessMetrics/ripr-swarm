<!-- section: Fixed -->
- Bounded help-proof capture failures retain their operation, path, I/O kind
  and OS error. Spawn failures name the command and scratch directory without
  dumping environment values, so real cleanup failures can be investigated
  without inferring a cause from a bare error (#7545).
