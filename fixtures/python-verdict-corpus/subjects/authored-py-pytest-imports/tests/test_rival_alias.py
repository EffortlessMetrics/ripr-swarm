from shop.other import quote as total


def test_rival_total_multiplies():
    assert total(100, 5) == 500
