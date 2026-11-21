<!-- section: Fixed -->
- Python: a test that calls a same-file helper returning the owner's call (`def parse(text): return parse_lines(text)`, then `assert parse(...) == ...`) now relates to the owner, so a change there no longer reads `no_static_path` and says no test reaches it. The link is reach-only, so it reads weak rather than `exposed` (#15350).
