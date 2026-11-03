<!-- section: Fixed -->
- Python collection config now follows pytest inifile precedence within a
  directory, so a stale lower-precedence file such as `setup.cfg` no longer
  withholds credit from a test the selected `pytest.ini` would collect (#12138).
