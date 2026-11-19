<!-- section: Fixed -->
- `ripr check --diff` reads a patch file git wrote under
  `diff.mnemonicPrefix=true` (sides named `i/`, `w/`, `c/` or `o/`) the same
  as the default `a/`/`b/` patch. It used to report every changed file as
  absent from the working tree and produce no findings. Prefix-free patches
  under a real directory with one of those names keep their reading (#15103).
