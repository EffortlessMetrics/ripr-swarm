<!-- section: Changed -->
- `ripr first-pr` no longer ends in "no actionable gap / stop" when the review cards report a weak changed-line finding with no machine repair start; it hands over the one `ripr check --root ... --base ...` command that names the missing check. `first-pr --check` now fails (blocked) for such a PR instead of passing as no-action (#11720).
