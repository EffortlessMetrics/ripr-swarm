<!-- section: Fixed -->
- A fractional `ripr.gitTimeoutMs` or `ripr.refreshDeadlineMs` no longer makes the language server discard every other ripr setting; both are integer settings and non-integer values fall back to the server default (#9828).
