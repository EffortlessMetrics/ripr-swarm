<!-- section: Fixed -->
- `ripr check` no longer prints `ripr progress: failed [diff]` before the real
  error when the input is user-fixable (an unresolvable `--base`, a missing
  `--diff` file, or an unusable root or config); genuine analysis failures
  still project `failed` (#9922).
