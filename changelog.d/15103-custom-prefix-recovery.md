<!-- section: Fixed -->
- `ripr check --diff` with a patch written with custom side prefixes (for
  example `--src-prefix=old/ --dst-prefix=new/`) still reports the changed file
  as absent, but when the same path without its first component is a regular
  source file under the root, the limitation now names the likely prefix and
  the `git diff --src-prefix=a/ --dst-prefix=b/` (or `--no-prefix`) fix
  alongside the checkout recovery (#15103).
