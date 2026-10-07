<!-- section: Fixed -->
- `ripr reports index --root <dir>` recorded the root but printed
  `--root .` in every regeneration command, so a pasted command analyzed
  the current directory instead of the indexed repository. For a
  non-default root the commands now name the resolved repository and give
  their artifact paths as absolute paths under the directory the index
  read; the default `--root .` output is unchanged (#4000).
