<!-- section: Added -->
- Python related-test rows now carry the `no_assertion` miss when the test calls
  the changed owner and its assertion admission is
  `extraction_complete_no_assertion` (#5491, RIPR-SPEC-0224). The reason is
  withheld, leaving `miss: null`, for a skipped or xfail test, a bare sibling
  helper module beside the test, and any conftest autouse fixture, hook,
  plugin, built-in fixture override or unreadable conftest on the directory
  chain. Admission now also treats names imported from assertion libraries
  (`nose`, `pytest_check`, `pytest_assume`, `hamcrest`, `pytest_*` plugins),
  imported decorators, `subprocess.run` and `unittest.main` as assertion-like.
  Plugins or `filterwarnings` set in ini files, `-p` and `addopts` stay
  invisible limits; the reason is explanation only and changes no class.
