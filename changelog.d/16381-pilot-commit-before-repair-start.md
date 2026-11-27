<!-- section: Fixed -->
- `ripr pilot` no longer recommends `ripr agent repair ... --phase before` as
  the first step while tracked production files are uncommitted, a state in
  which that start refuses ("commit first ... Nothing was started"). The
  terminal and the saved Markdown summary now name the commit
  (`git -C <root> --literal-pathspecs commit ...`) before the repair start,
  from the same dirty-premise check the start applies. Staged and unstaged
  edits are both covered; a test file you are still editing adds no commit
  step (#16381).
