<!-- section: Added -->
- `ripr_get_receipt_status` (MCP) now carries the same `test_run` object as `ripr agent status --attempt <id> --json` for a finished attempt: `caller_reported` with the counts when the finish recorded a `--test-result`, naming a reported failure, or `not_recorded`. An MCP client no longer sees `improved` without the run it reported. ripr still runs nothing (#16251).
