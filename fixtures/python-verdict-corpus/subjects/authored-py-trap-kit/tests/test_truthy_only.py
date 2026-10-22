from trapkit.core import truthy_only


def test_truthy_only():
    assert truthy_only(3, 7)
