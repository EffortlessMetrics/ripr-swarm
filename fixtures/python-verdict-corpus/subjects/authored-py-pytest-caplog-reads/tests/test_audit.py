import logging

from reads.audit import (
    expiring,
    frozen,
    locked,
    low_balance,
    low_stock,
    overdrawn,
    slow_query,
)


def test_low_balance_is_a_captured_message(caplog):
    with caplog.at_level(logging.WARNING):
        assert low_balance(5) is False
    assert "low balance: 5" in caplog.messages


def test_low_stock_message_is_the_whole_text(caplog):
    with caplog.at_level(logging.WARNING):
        assert low_stock(2) is False
    assert "low stock: 2" in caplog.messages


def test_overdrawn_record_contains_the_text(caplog):
    with caplog.at_level(logging.WARNING):
        assert overdrawn(-1) is False
    assert any("overdrawn" in r.message for r in caplog.records)


def test_frozen_record_message_equals(caplog):
    with caplog.at_level(logging.WARNING):
        assert frozen("frozen") is False
    assert any(r.getMessage() == "account frozen: frozen" for r in caplog.records)


def test_locked_record_contains_the_text(caplog):
    with caplog.at_level(logging.WARNING):
        assert locked("locked") is False
    assert any("account locked" in r.message for r in caplog.records)


def test_slow_query_either_wording(caplog):
    with caplog.at_level(logging.WARNING):
        assert slow_query(900) is False
    assert "slow query: 900" in caplog.messages or "query slow: 900" in caplog.messages


def test_expiring_message_in_record_list(caplog):
    with caplog.at_level(logging.WARNING):
        assert expiring(3) is False
    assert "card expiring: 3" in [r.message for r in caplog.records]
