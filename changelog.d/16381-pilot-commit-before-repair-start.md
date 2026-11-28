<!-- section: Fixed -->
- `ripr pilot` no longer recommends `ripr agent repair ... --phase before` as
  the first step while tracked production files are uncommitted, a state in
  which that start refuses ("commit first ... Nothing was started"). The
  terminal and the saved Markdown summary now name the commit
  (`git -C <root> --literal-pathspecs commit ...`) before the repair start,
  for the files the start's own dirty-premise preflight names. Staged and
  unstaged edits are both covered; a test file you are still editing adds no
  commit step. Under a subdirectory root, with a dirty file outside the
  analyzed root, or when the premise cannot be read, pilot prints `ripr
  check`'s "no repair start offered" line instead of a commit and a start. The
  preflight skips deleted files and files carrying an
  inline `#[cfg(test)]` module, so the start can still refuse those after the
  commit (#16381).
