from trapkit.core import named_test


def test_named_total():
    assert named_test(3, 7) == 21
