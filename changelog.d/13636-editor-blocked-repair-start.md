<!-- section: Fixed -->
- Editor: while the production change is uncommitted, "Start repair: copy
  repair command" is listed disabled with the commit step
  (`git -C <root> --literal-pathspecs commit ...`) instead of offering a start
  that refuses, and the seam hover names that commit before the start.
  Committing re-enables it without a refresh. In VS Code, "ripr: Start Current
  Repair" now shows the server's reason instead of "Refresh saved-workspace
  analysis", and also works from a seam diagnostic with no gap record (#13636).
