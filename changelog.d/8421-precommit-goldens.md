<!-- section: Changed -->
- `cargo xtask precommit` now runs `goldens check` when the diff touches output-surface code or fixture goldens, so stale `human.txt`/`check.json` goldens fail locally instead of turning main red. Other diffs skip it, and fixtures that only lack prepared dependencies are reported as an environment gap rather than a pass or drift (#8421).
