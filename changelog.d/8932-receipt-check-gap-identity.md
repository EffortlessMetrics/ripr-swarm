<!-- section: Fixed -->
- `ripr receipt check` now rejects a receipt whose present `canonical_gap_id` is null, non-string, empty or blank (exit 2, empty stdout under `--json`, field-specific stderr) instead of reporting a structurally valid `not_available` result (#8932).
