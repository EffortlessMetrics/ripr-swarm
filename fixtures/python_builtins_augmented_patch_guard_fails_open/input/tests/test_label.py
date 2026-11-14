from src.label import label


def test_int_fallthrough():
    assert label(3) == None
