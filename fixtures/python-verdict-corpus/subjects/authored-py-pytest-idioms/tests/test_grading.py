import pytest

from idioms.grading import grade


@pytest.mark.parametrize(
    "score,expected", [(95, "A"), (85, "B"), (10, "C")], ids=["top", "mid", "low"]
)
def test_grade(score, expected):
    assert grade(score) == expected
