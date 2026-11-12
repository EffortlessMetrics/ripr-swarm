<!-- section: Fixed -->
- Python: a conftest fixture that returns `Class([3, 4])` (a list, tuple, set or dict display of literals as an argument) is now read like the inlined construction, as `Class(3)` already was, instead of stopping at `unresolved_pytest_fixture`. A display that names a variable, calls something, stars an element or is a comprehension keeps the limit. (#14280)
