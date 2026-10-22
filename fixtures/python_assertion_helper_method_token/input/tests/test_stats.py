from src.stats import Stats

def check_writes(obj, want):
    assert obj.record_write == want

def test_writes():
    stats = Stats()
    stats.record_write()
    method = stats.record_write
    check_writes(stats, method)
