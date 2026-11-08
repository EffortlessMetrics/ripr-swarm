<!-- section: Added -->
- `ripr check` honors cargo-mutants' `#[mutants::skip]` (and
  `#[cfg_attr(<predicate>, mutants::skip)]`) on a changed owner function or
  method as an explicit owner opt-out. Its findings keep their class but are
  withheld from the human listing, triage and GitHub annotations, and the
  withholding is disclosed: human output names the count, owner and the
  `owner_mutants_skip` reason, and JSON adds `withheld_by` on each such
  finding, a top-level `owner_opt_outs` object and a
  `summary.withheld_by_owner_opt_out` count (#13819).
