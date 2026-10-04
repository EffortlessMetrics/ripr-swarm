<!-- section: Fixed -->
- Repository text with control or bidi characters no longer reaches the
  terminal raw through `--format github` annotations, command-failure lines
  on stderr, or the stderr warnings that quote a config value, path or ref
  (#6309). Printed drill-in commands spell such characters as bash `$'\xHH'`
  escapes, so a pasted `ripr explain --root ...` still names the same
  directory.
