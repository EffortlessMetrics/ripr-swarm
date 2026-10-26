from unrun.core import dead_while


def test_dead_while():
    total = dead_while(3, 7)
    while False:
        assert total == 21
