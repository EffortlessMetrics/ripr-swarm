from src.stats import Stats, Stats as OtherStats

def check_writes(obj, want):
    assert obj.writes == want

def test_writes():
    a = Stats()
    b = OtherStats()
    a.record_write()
    check_writes(b, 0)
    b.record_write()
