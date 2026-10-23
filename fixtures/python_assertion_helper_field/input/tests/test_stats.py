from src.stats import Stats

def check_writes(obj, want):
    assert obj.writes == want

def test_writes():
    stats = Stats()
    stats.record_write()
    stats.record_write()
    check_writes(stats, 2)
