<!-- section: Fixed -->
- `ripr pilot` now renders the loaded config path through the shared
  human-path renderer on the terminal and in `pilot-summary.md`, so on
  Windows the canonicalized `ripr.toml` path no longer leaks the raw `\\?\`
  verbatim prefix — the same spelling the check header and doctor already
  show (#8696).
