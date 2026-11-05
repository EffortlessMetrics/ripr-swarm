<!-- section: Internal / CI -->
- `cargo xtask eval-sweep refresh` retains exact output and diagnostic digests
  from non-complete repeats without granting stability credit. Repeat-output
  write failures preserve retained first-pass run evidence and keep comparison
  unavailable, so losing repeat evidence cannot improve the sweep gate (#13140).
