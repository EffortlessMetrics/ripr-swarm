<!-- section: Fixed -->
- `ripr check` on an empty diff no longer advises `--base <branch>` when
  that branch's diff would itself refuse as `diff_scope_oversized`; when
  several branches qualify it names the one with the smallest analyzable
  diff instead of the first in ref order (#9013).
