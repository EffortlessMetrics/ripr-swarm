<!-- section: Fixed -->
- `ripr reports index --root <dir>` recorded the root but printed
  `--root .` in every regeneration command, so a pasted command analyzed
  the current directory instead of the indexed repository. The commands
  now name the root the index was built for, in its typed spelling; the
  default `--root .` output is unchanged (#4000).
