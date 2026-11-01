<!-- section: Fixed -->
- The Python repair edit cage now exempts interpreter and test-runner cache paths before the 10 000-path capture cap, so large ignored `__pycache__` trees no longer make the cage incomparable (#10801). The exemption set also includes `.hypothesis/`, `htmlcov/`, and root `coverage.xml`; tracked cache-shaped files and importable paths inside cache directories stay observed.
