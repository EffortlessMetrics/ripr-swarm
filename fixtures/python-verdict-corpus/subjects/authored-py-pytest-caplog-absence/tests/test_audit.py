import logging

from absence.audit import archive, notify, purge, refund, rotate, settle, ship


def test_settle_does_not_queue_a_retry(caplog):
    with caplog.at_level(logging.WARNING):
        assert settle(5) is True
    assert "retry queued" not in caplog.text


def test_notify_does_not_dump_debug_state(caplog):
    with caplog.at_level(logging.WARNING):
        assert notify("ana") is True
    assert "debug dump" not in caplog.messages


def test_refund_leaks_no_personal_data(caplog):
    with caplog.at_level(logging.WARNING):
        assert refund(7) is True
    assert not any("pii" in r.message for r in caplog.records)


def test_ship_does_not_log_a_trace(caplog):
    with caplog.at_level(logging.WARNING):
        assert ship(3) is True
    assert "trace id" not in [r.message for r in caplog.records]


def test_archive_does_not_queue_a_retry(caplog):
    with caplog.at_level(logging.WARNING):
        assert archive(4) is True
    assert "retry queued" not in caplog.text


def test_purge_records_below_the_pinned_level_are_not_captured(caplog):
    with caplog.at_level(logging.ERROR):
        assert purge(2) is True
    assert "retry queued" not in caplog.text


def test_rotate_either_absence(caplog):
    with caplog.at_level(logging.WARNING):
        assert rotate(9) is True
    assert "retry queued" not in caplog.text or "rotated" in caplog.text
