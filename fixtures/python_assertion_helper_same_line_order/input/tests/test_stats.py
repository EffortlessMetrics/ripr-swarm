from src.stats import Stats

def check_writes(obj, want):
    assert obj.writes == want

def test_writes():
    stats = Stats()
    check_writes(stats, 0); stats.record_write()
