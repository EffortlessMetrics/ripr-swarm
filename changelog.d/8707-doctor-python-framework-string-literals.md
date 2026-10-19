<!-- section: Fixed -->
- `ripr doctor`'s Python framework evidence scan no longer counts
  `import unittest` / `import pytest` lines inside docstrings and
  triple-quoted string literals, so a non-test module admitted by file name
  (e.g. Django-conventional `tests.py`) whose example text names a framework
  is no longer reported as a confirmed test framework (#8707).
