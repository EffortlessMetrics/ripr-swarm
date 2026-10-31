<!-- section: Fixed -->
- `ripr.toml` `[languages] enabled` now accepts `javascript` as an alias for
  `typescript`, matching the language label ripr already emits for
  JavaScript files instead of rejecting it as an unknown language; listing
  both names is refused as a duplicate (#11133).
