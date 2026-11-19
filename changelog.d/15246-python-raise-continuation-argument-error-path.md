<!-- section: Fixed -->
- Python: a changed non-literal argument (an f-string, `"msg" % x`, a name) on its own line of a multi-line `raise Exc(` is now classified as the raise's error path, so a success-only exact test no longer makes it a false `exposed`; the one-line and plain-literal shapes already agreed (#15246).
