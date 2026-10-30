from trapkit.core import result_asserted


def test_result_asserted():
    a, b = 3, 7
    result = result_asserted(a, b)
    assert result == 21
