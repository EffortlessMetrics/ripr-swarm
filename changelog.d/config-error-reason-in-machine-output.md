<!-- section: Fixed -->
- Machine-readable config errors (`--json`, SARIF, GitHub, editor) now carry the parser's reason, such as `unknown key, expected one of ...` or `unclosed table, expected `]``, not just the line and column; file content still never enters them (#14441).
