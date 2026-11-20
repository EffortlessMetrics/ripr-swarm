<!-- section: Fixed -->
- `ripr gate check --root <dir>`, `ripr gate evaluate --root <dir>` and
  `ripr review-comments --root <dir>` now write their default reports under
  `<dir>/target/ripr/...`, where `ripr reports ci-summary --root <dir>` and the
  gate report's own hints look, instead of under the invoking directory. An
  explicit relative `--out`/`--guidance-out` still resolves against the
  invoking directory (#15378).
