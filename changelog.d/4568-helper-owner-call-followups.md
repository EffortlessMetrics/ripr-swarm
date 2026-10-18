<!-- section: Fixed -->
- Python: the `helper_owner_call` relation (#4568) now ignores decorated callers (including unreadable decorators) and shares its call-site test with direct owner calls; direct test-to-owner call detection now also ignores call text inside docstrings and string literals.
