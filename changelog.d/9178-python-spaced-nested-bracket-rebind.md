<!-- section: Fixed -->
- Python: a protocol-entry receiver rebound through a spaced or nested bracketed target (`( r), x = ...`, `((r), x) = ...`, `for ((r), x) in ...`) is no longer credited `exposed`; reads such as `self.assertEqual(len(r), 0)` still credit (#9178).
