<!-- section: Fixed -->
- `ripr doctor` no longer reports "test framework not detected" for repos whose unittest or pytest tests live in Django-conventional `tests.py` / `test.py` modules or one level inside app directories; framework detection now reuses the same Python test-path predicate as the test index (https://github.com/EffortlessMetrics/ripr-swarm/issues/7706).
