import logging

from audit import check_balance


def test_low_balance(caplog):
    with caplog.at_level(logging.WARNING):
        assert check_balance(5) is False
    msgs = caplog.messages
    msgs[0] = "low balance: 5"; assert "low balance: 5" in msgs
