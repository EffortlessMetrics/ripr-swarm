from src.stats import Stats

def check_writes(obj, other):
    assert obj.writes == other.writes


def test_writes():
    stats = Stats()
    stats.record_write()
    other = stats
    check_writes(stats, other)
