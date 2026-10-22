<!-- section: Fixed -->
- Python: a changed `self.attr = ...` line under an `if`, `try` or loop in an ordinary method is no longer credited as an observable effect, because the test may never run it (#9782).
