from discount import bulk_discount


def test_bulk_discount_at_threshold(quantity): assert bulk_discount(quantity) == 0.1
