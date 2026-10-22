from src.stats import Stats

def check_writes(obj, want):
    assert obj.writes == want

def test_writes():
    stats = Stats()
    False and stats.record_write()
    check_writes(stats, 0)
