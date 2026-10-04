import pytest

import pricing
from pricing import (
    bulk_price,
    discount_cents,
    label_for,
    loyalty_points,
    parse_quantity,
    quote,
    shipping_fee,
    tier_for_points,
    total_with_tax,
)
from pricing import tax_cents


def test_free_shipping_starts_at_threshold():
    assert shipping_fee(5000) == 0
    assert shipping_fee(4999) == 499


def test_tiers_far_from_thresholds():
    assert tier_for_points(6000) == "gold"
    assert tier_for_points(0) == "bronze"


@pytest.mark.parametrize("points,tier", [(999, "bronze"), (1000, "silver")])
def test_silver_starts_at_one_thousand(points, tier):
    assert tier_for_points(points) == tier


def test_gold_members_get_a_discount():
    assert discount_cents("gold", 1000) > 0


def test_total_includes_tax():
    assert total_with_tax(1000) == 1000 + tax_cents(1000)


def test_zero_quantity_is_rejected():
    with pytest.raises(ValueError):
        parse_quantity("0")


def test_points_are_whole_numbers():
    assert isinstance(loyalty_points(1050), int)


def test_label_is_not_the_raw_tier():
    assert label_for("gold") != "gold"


def test_bulk_price_with_explicit_threshold():
    assert bulk_price(100, 5, bulk_threshold=5) == 450
    assert bulk_price(100, 4, bulk_threshold=5) == 400


def test_quote_for_a_large_bronze_order():
    assert quote(6000, 100)["total_cents"] == 6000


def test_receipt_prints_dollars(capsys):
    pricing.print_receipt(1250)
    assert capsys.readouterr().out == "Total: 12.50\n"
