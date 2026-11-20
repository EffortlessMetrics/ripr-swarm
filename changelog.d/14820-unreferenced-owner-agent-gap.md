<!-- section: Fixed -->
- `ripr check --format agent` no longer reports "ripr cannot tell" with
  `gaps=0` when a changed function is called by nothing in the workspace.
  Such findings now count as gaps; when no missing check value is known, the
  row names the function (``no test calls `compare` ``) and points at a new
  test. Findings ripr genuinely cannot trace stay listed as limits (#14820).
