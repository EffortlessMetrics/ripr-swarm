<!-- section: Fixed -->
- Artifact report commands (`reports gap-ledger|ts-limitations|ts-false-actionable`,
  `outcome`, `calibrate cargo-mutants`, `baseline create|diff|update`,
  `zero status`, `evidence-health`, `pr-ledger record`, `pr-comments plan`,
  `pr-review front-panel`, `coverage-grip frontier`, `first-action`,
  `assistant-loop proof|health`, and every `policy` report) now refuse, with
  exit `2` and before writing anything, an `--out` or `--out-md` that names
  the same file as one of their inputs (also through an absolute spelling or a
  symlink). Before, the input was silently replaced. `baseline update --out`
  naming `--baseline` is still the documented in-place update (#14312).
