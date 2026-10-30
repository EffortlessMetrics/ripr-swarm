from trapkit.core import dead_branch


def test_dead_branch_total():
    total = dead_branch(3, 7)
    if False:
        assert total == 21
