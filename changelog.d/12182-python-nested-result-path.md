<!-- section: Fixed -->
- Python: a strong assertion on an enclosing function no longer gives a changed `return` in a nested function direct return-value credit when the enclosing body calls the nested function only as a statement of its own, so its result reaches nothing the test asserts; the finding is `weakly_exposed` (#12182).
