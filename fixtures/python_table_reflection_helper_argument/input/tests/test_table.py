import rates.table as table
from rates.table import price
from rates.util import poke

poke(table, "RATES", "silver", 5)


def test_silver_price():
    assert price("silver") == 5
