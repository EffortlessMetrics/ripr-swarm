<!-- section: Fixed -->
- `ripr config validate` now fails when an exact-path key (`analysis.production_like_targets`, `analysis.test_harnesses[].target`, `languages.*.handwritten_files`) names no file under the root, listing every missing entry with its key; analysis had treated each such typo as a setting that matched nothing (#15495).
