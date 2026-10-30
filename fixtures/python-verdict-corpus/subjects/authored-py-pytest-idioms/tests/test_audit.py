import logging

from idioms.audit import check_balance


def test_low_balance_logs_a_warning(caplog):
    with caplog.at_level(logging.WARNING):
        assert check_balance(5) is False
    assert "low balance: 5" in caplog.text
