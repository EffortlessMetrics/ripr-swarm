from uncollected.core import conftest_ignored


def test_conftest_ignored():
    assert conftest_ignored(3, 7) == 21
