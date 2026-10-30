from trapkit.core import live_branch


def test_live_branch_total():
    total = live_branch(3, 7)
    if True:
        assert total == 21
