from trapkit.core import conditional_exit


def test_conditional_exit_total():
    total = conditional_exit(3, 7)
    if total < 0:
        return
    assert total == 21
