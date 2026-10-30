from src.stats import Stats

def check_writes(obj, want):
    assert obj.writes == want

def test_writes():
    stats = Stats()
    stats.record_write()
    stats.record_write()
    other = Stats()
    check_writes(other, 0)
