from src.stats import Stats

def check_writes(obj, want):
    assert obj.writes == want

def drop(pending):
    pending.close()

def test_writes():
    stats = Stats()
    drop(stats.record_write())
    check_writes(stats, 0)
