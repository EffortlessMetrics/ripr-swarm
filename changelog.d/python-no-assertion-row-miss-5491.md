<!-- section: Added -->
- Python related-test rows now carry the `no_assertion` miss when the test calls
  the changed owner and its assertion admission is
  `extraction_complete_no_assertion` (#5491, RIPR-SPEC-0224). The reason is
  withheld, leaving `miss: null`, for a skipped or xfail test, a bare-importable
  helper module beside the test or in its pytest rootdir, module-level effects
  in the test module (filters, skips, asserts, star imports), and any conftest
  on the directory chain that declares a fixture or hook, has top-level calls,
  or cannot be read. Admission now also treats names imported from assertion libraries
  (`nose`, `pytest_check`, `pytest_assume`, `hamcrest`, `pytest_*` plugins),
  imported decorators and context managers, `pytest.param(..., marks=...)`, `request.applymarker`, `subprocess.run`, `unittest.main`, `doctest` and `importlib.import_module` as assertion-like.
  Plugins or `filterwarnings` set in ini files, `-p` and `addopts` stay
  invisible limits; the reason is explanation only and changes no class.
