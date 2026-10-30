<!-- section: Fixed -->
- Rust boundary-input evidence now keeps a nested owner re-export's equality input
  beside an unrelated external sibling. External, renamed (including `as _`) and
  `cfg`/`cfg_attr`-gated leaves remain refused; explicit dependency bindings still
  beat owner globs. This repairs infection-stage input evidence; separate static
  limitations still govern final classifications and actionable gaps (#10047).
