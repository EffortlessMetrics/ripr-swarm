<!-- section: Fixed -->
- Python: a `pytest.raises(..., match=...)` or `assertRaisesRegex` test whose
  body calls the changed function through its module (`levels.parse("")`) now
  credits a changed `raise` like the same test written with the imported name
  (`parse("")`). Before, it read `weakly_exposed` and the next step asked for
  the exception check the test already held (#14090).
