<!-- section: Fixed -->
- Python bare-bool assert credit is no longer switched off by any `mock` / `patch` substring in the test file: patching that does not name the callee (`monkeypatch.setenv`, `mock.patch.dict`, patching other names) keeps the exact-pin credit, while a patch that names the callee or whose target cannot be read still leaves the assertion smoke (#10391).
