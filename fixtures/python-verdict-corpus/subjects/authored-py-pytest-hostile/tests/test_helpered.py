from hostile.helpered import helpered_square
from tests.helpers import check_equal


def test_helpered_square():
    check_equal(helpered_square(3), 9)
