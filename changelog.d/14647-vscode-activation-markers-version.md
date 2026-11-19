<!-- section: Fixed -->
- VS Code: the extension now also activates in workspaces whose only project marker is
  `setup.py`, `setup.cfg`, `requirements.txt`, `pytest.ini`, `tox.ini` or `noxfile.py`,
  matching the markers it analyzes; the expected server version always comes from the
  extension's own manifest instead of a fixed `0.8.0` fallback; and `@types/vscode`
  matches the declared minimum VS Code 1.91 (#14647).
