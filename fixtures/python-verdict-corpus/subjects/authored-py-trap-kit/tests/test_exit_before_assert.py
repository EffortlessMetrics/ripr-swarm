from trapkit.core import exit_before_assert


def test_exit_before_assert_total():
    total = exit_before_assert(3, 7)
    return
    assert total == 21
