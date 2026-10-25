<!-- section: Fixed -->
- Rust boundary-input evidence now keeps a nested owner re-export's equality input
  beside an unrelated external sibling, closing the missing-boundary gap. External,
  renamed (including `as _`) and `cfg`/`cfg_attr`-gated leaves keep the gap; explicit
  dependency bindings still beat owner globs (#10047).
