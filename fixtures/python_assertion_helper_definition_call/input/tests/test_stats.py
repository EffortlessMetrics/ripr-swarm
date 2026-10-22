from src.stats import Stats

def check_writes(obj, want):
    assert obj.writes == want

stats = Stats()

def test_writes(unused=stats.record_write()):
    stats = Stats()
    check_writes(stats, 0)
