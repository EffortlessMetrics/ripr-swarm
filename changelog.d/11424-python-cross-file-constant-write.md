<!-- section: Fixed -->
- A Python test-module constant is no longer credited as a boundary input
  when another workspace file writes that module's name (`conftest.py`
  doing `module.THRESHOLD = 49`, `setattr`, or `monkeypatch.setattr`); an
  unrelated write keeps the credit (#11424).
