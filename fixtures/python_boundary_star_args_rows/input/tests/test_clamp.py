import pytest

from fmt.clamp import clamp


@pytest.mark.parametrize(
    "args, expected",
    [
        ([1], "1"),
        ([0.0001, 0.01], "<0.01"),
        ([5, 1], "5"),
    ],
)
def test_clamp(args, expected):
    assert clamp(*args) == expected
