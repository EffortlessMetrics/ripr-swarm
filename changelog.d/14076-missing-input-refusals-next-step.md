<!-- section: Fixed -->
- `ripr zero status`, `ripr calibrate cargo-mutants`, `ripr cache export`, `ripr pr-ledger record`, `ripr pr-review front-panel` and `ripr reports gap-ledger` no longer stop at "requires X" when a required input is missing: each refusal now names where the value comes from or a concrete example to pass. Exit `2` on stderr is unchanged (#14076).
