<!-- section: Changed -->
- `ripr first-pr` no longer ends in "no actionable gap / stop" when the review cards report a weak changed-line finding with no machine repair start; it hands over the one `ripr check --root ... --base ...` command that names the missing check (#11720).
