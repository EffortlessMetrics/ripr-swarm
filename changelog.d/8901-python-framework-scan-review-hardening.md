<!-- section: Fixed -->
- `ripr doctor`'s Python framework evidence scan no longer treats `"""` /
  `'''` inside comments or ordinary single-line strings as string
  delimiters, so a line like `# use """ for docstrings` or `x = "'''"`
  cannot swallow later real `import pytest` / `import unittest` lines and
  hide a genuine test framework, and directories named `test_*.py` no
  longer count toward the per-directory scan cap ahead of real test files
  (#8706, #8707; PR #8901 review).
