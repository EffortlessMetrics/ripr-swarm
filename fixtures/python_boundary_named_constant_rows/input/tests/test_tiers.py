import pytest

from fmt.tiers import tier

LOW = 20
HIGH = 250


@pytest.mark.parametrize(("points", "expected"), [(LOW, "basic"), (HIGH, "gold")])
def test_tier(points, expected):
    assert tier(points) == expected
