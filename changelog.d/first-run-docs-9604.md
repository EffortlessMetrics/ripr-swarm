<!-- section: Docs -->
- The Bash prebuilt-archive install snippet in the quickstart now fails with a
  clear message on unsupported platforms instead of letting `curl` print a 404
  for an empty URL (#9604).
- README and quickstart now say that the shown `PATH` change lasts only for
  the current shell or PowerShell session, and how to make it permanent
  (#9604).
- The README quickstart now says up front that its first section uses the
  published 0.10 release while the output at the top of the page, `--worktree`
  and the repair flow need the 0.11 development build (#9604).
- README and quickstart now note that `ripr doctor` exits 3 when any check
  fails, so it does not chain with `&&` (#9604).
