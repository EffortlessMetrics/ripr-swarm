<!-- section: Fixed -->
- Python: a pytest fixture alias (`from pytest import fixture as fx`) that an assignment, `def` or `class` rebinds before the decorated def is no longer read as the fixture decorator, so a `caplog` defined under it no longer withholds the built-in capture evidence and it no longer claims fixture reach (#10473).
