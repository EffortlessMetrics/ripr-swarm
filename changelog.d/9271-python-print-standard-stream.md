<!-- section: Changed -->
- Python: `print(..., file=sys.stderr)` and `file=sys.stdout` no longer read as an unresolved import, so an exact `capsys.readouterr().err` assertion credits an edit to the printed text; `py-fixtures-warning-stderr` moves from abstained to credited. Other `sys` uses still abstain (#9271).
