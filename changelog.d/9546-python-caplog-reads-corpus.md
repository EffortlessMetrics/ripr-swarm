<!-- section: Internal / CI -->
- Verdict corpus: seven Python cases (`authored-py-pytest-caplog-reads`) pin the `caplog.messages` and `caplog.records` reads against real pytest runs: five edits the tests catch, one records substring search that an appended suffix escapes, and one `or` of two wordings (#9546).
