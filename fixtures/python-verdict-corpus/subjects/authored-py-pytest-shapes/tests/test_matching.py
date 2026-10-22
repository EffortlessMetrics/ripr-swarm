from shapes.matching import kind


def test_zero_and_int():
    assert kind(0) == "zero"
    assert kind(5) == "int"
