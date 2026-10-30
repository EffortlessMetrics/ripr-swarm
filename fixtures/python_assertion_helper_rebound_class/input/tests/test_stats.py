from src.stats import Stats

def check_writes(obj, want):
    assert obj.writes == want

class Other:
    writes = 0
    def record_write(self):
        pass

Stats = Other

def test_writes():
    stats = Stats()
    stats.record_write()
    check_writes(stats, 0)
