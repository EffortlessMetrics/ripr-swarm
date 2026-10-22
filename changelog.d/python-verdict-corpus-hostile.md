<!-- section: Internal / CI -->
- Python verdict corpus: new `authored-py-pytest-hostile` subject and 20 cases
  labeled against real pytest runs (non-ASCII names, tabs, no final newline,
  very long lines, builtin-named functions, conditional definitions, star
  imports, CRLF test file with a helper assertion, `globals()` dispatch,
  same-name functions in two modules). ripr scores them with 5 false
  actionable verdicts, each tracked under #9412 until its fix lands (#9413).
