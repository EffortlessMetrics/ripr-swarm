<!-- section: Fixed -->
- Python: a non-mock related test that exactly pins a changed owner is credited independently of `mock.patch` / `monkeypatch` siblings instead of leaving the finding `static_unknown` (`mocked_module`); a mock-only test set, a sibling whose patch is started and never undone, and a non-mock test that does not itself earn `exposed` keep the limit (#13633).
