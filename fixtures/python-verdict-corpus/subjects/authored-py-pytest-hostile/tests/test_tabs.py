from hostile.tabs import tabbed_total


def test_tabbed_total():
    assert tabbed_total([1, 2, 3]) == 6
