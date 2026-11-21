<!-- section: Fixed -->
- Baselines: a `#[mutants::skip]` added to an owner after a baseline was
  recorded no longer reads that owner's baseline debt as `resolved` in
  `ripr baseline diff`, and `ripr baseline update --remove-resolved` no
  longer drops it. The entry reads `suppressed` with
  `"withheld_by": "owner_mutants_skip"`, and the Markdown delta lists it
  under "Withheld by owner opt-out". Baseline entries now record their
  `changed_owner`, and `gate evaluate --gap-ledger` carries the ledger's
  `owner_opt_outs` into `analysis_scope`. A baseline written before this
  change reads such entries as `not_evaluated`, never `resolved`; re-create
  it to restore the match (#14898).
