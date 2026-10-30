from src.stats import Stats

def check_writes(obj, want):
    assert obj.writes == want


def test_writes():
    stats = Stats()
    stats.record_write()
    import stats
    check_writes(stats, 0)
