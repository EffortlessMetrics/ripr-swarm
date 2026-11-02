from hostile.nested_effect import collect


def test_collect():
    assert collect() == [2]
