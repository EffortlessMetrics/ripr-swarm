<!-- section: Fixed -->
- `ripr context <selector>` now names the flag-only grammar instead of a
  bare unknown-argument error: the refusal points at `ripr context --at
  <selector>` (or `--finding`) and does not accept a positional selector
  (#8745).
