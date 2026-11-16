<!-- section: Fixed -->
- `ripr check --format agent` no longer reports "ripr cannot tell" with
  `gaps=0` when a changed function is called by nothing in the workspace. Each
  such finding is now a gap row naming the function (``no test calls
  `compare` ``) and pointing at a new test, while findings ripr genuinely
  cannot trace stay listed as limits (#14820).
