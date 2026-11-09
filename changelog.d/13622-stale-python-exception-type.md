<!-- section: Fixed -->
- Python: a stale `pytest.raises(OldExc, match=...)` no longer closes a gap after the raised exception type changed; the finding stays `weakly_exposed` and names the new type (#13622).
