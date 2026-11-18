<!-- section: Fixed -->
- The repo-scope formats of `ripr check` (`repo-exposure-*`, `repo-seams-*`,
  `repo-badge*`) now withhold a seam whose owner sits inside a
  `#[mutants::skip]` `impl`, `trait` or inline module, or in a file that
  starts with `#![mutants::skip]`, and disclose it in `owner_opt_outs` like a
  fn-level opt-out. The decision is the one diff-scope `ripr check` already
  applies to findings, so both scopes withhold the same owners (#15000).
