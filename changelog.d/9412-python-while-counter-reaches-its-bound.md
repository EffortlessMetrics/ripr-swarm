<!-- section: Fixed -->
- Python: a `while` bound compared against a parameter that the loop body steps by one (`while start > 0:` with `start -= 1`) is reached by any test input past the bound, so a changed bound is credited when an exact test drives the loop through it (`py-shapes-generator-bound`, #9412).
