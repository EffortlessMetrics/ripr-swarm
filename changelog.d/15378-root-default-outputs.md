<!-- section: Fixed -->
- `ripr gate check --root <dir>`, `ripr gate evaluate --root <dir>` and
  `ripr review-comments --root <dir>` now write their default reports under
  `<dir>/target/ripr/...`, where `ripr reports ci-summary --root <dir>` and the
  gate report's own hints look, instead of under the invoking directory. An
  explicit relative `--out`/`--guidance-out` still resolves against the
  invoking directory (#15378).
- Review cards (`comments.md`) now print a PowerShell line under each card
  command that PowerShell cannot run as written, from the same translator the
  terminal output uses (#15378).
