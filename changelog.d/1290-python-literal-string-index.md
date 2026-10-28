<!-- section: Fixed -->
- An undecorated synchronous Python function with only an optional docstring and a literal-string return no longer gains `exposed` credit from a direct equality against a string constant when a bare owner-name call with only positional AST constant arguments indexes the same unchanged character in both versions (#1290).
