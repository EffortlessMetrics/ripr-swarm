from src.stats import Stats

def check_writes(obj, want):
    assert obj.writes == want


def test_writes():
    stats = Stats()
    stats.record_write()
    want = stats.writes
    check_writes(stats, want)
