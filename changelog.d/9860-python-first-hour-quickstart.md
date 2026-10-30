<!-- section: Added -->
- `docs/QUICKSTART.md` gains a "Python first hour (preview)" section: the
  project markers that auto-enable the Python preview (`pyproject.toml`,
  `setup.py`, `setup.cfg`, `requirements.txt`, `pytest.ini`, `tox.ini`,
  `noxfile.py`), the expected `ripr check` / `ripr pilot --root .` flow with
  the advisory-evidence caveat, the `ripr.toml` `[languages] enabled =
  ["python"]` fallback quoted from the runtime message for marker-less
  repositories, and a reminder to ignore `target/ripr/`. README's Status
  table and "What you get" table point Python users to it (#9860).
