<!-- section: Fixed -->
- Editor: while tracked source files are uncommitted, "Start repair: copy
  repair command" is listed disabled with the commit command
  (`git -C <root> --literal-pathspecs commit ...`) instead of offering a start
  that refuses, and the seam hover names that commit before the start.
  Committing re-enables it without a refresh. In VS Code, "ripr: Start Current
  Repair" shows that reason instead of opening another action or suggesting
  "Refresh saved-workspace analysis", and also works from a seam diagnostic
  with no gap record (#13636).
