from trapkit.core import input_asserted


def test_input_asserted():
    a, b = 3, 7
    result = input_asserted(a, b)
    assert a == 3
    assert b == 7
    assert result is not None
