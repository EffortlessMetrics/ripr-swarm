<!-- section: Fixed -->
- A returned comparison whose integer threshold moves by more than one
  (`x > 100` to `x > 102`) is no longer read as missing its boundary when a
  related test passes an intermediate input that separates the two versions
  (`thr(101)`); inputs on the same side of both thresholds keep the gap (#9147).
