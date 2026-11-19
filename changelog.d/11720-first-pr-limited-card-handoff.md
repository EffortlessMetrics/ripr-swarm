<!-- section: Changed -->
- `ripr first-pr` no longer ends in "no actionable gap / stop" when current review cards place a weak finding with no machine repair start on a changed line; it hands over one `ripr check --root ... --base ... --format human-full` command that shows the missing check for each. `first-pr --check` now fails (blocked) for such a PR instead of passing as no-action (#11720).
