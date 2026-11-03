<!-- section: Fixed -->
- Python: a nested function no longer counts as called by its enclosing function when the only call is from an unused def, class or lambda inside an `if`/`with`/`for`/`try` block, or when the name is defined more than once in the enclosing body; those shapes stay fail-closed (#12197).
