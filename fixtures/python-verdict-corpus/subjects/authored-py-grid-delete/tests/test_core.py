import pytest

from grid_delete.core import Stats


def test_one_hit_is_counted():
    stats = Stats()
    stats.record_hit()
    assert stats.hits == 1


@pytest.mark.parametrize("calls,want", [(0, 0), (1, 1), (3, 3)])
def test_miss_table(calls, want):
    stats = Stats()
    for _ in range(calls):
        stats.record_miss()
    assert stats.misses == want


def test_reads_never_go_negative():
    stats = Stats()
    for _ in range(3):
        stats.record_read()
    assert stats.reads >= 0


def check_writes(stats, want):
    assert stats.writes == want


def test_writes_are_counted():
    stats = Stats()
    stats.record_write()
    stats.record_write()
    check_writes(stats, 2)


def test_eviction_smoke():
    stats = Stats()
    stats.record_eviction()
