<!-- section: Internal / CI -->
- `cargo xtask kani` reports ambiguous discovered harness names and duplicate
  matching result blocks as NOT-ESTABLISHED instead of borrowing the first
  verdict or verification time; unrelated unique results retain their meaning
  (#12468).
