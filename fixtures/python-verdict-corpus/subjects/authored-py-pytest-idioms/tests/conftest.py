import pytest

from idioms import memo


@pytest.fixture(autouse=True)
def clean_memo():
    memo._cache.clear()
    yield
    memo._cache.clear()
