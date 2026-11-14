<!-- section: Fixed -->
- Python: a test that `monkeypatch.setattr`s a module-level callee with a literal stand-in no longer hides an edit in the caller that neither names nor feeds that callee (the exact assertion credits it), and a change inside the patched function reads `reachable_unrevealed` (every related test replaces it) instead of `weakly_exposed` with a heuristic-link note (#13632).
