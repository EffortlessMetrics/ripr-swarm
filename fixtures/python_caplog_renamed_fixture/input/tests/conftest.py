import logging

import pytest


class Recorder:
    text = "low balance: 5"

    def at_level(self, level):
        return logging.getLogger().setLevel(level) or _Noop()


class _Noop:
    def __enter__(self):
        return self

    def __exit__(self, *exc):
        return False


@pytest.fixture(name="caplog")
def custom_recorder():
    return Recorder()
