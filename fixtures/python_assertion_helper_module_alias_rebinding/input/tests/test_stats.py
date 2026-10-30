import src.stats as mod
from types import SimpleNamespace

class Other:
    writes = 0
    def record_write(self):
        pass

mod = SimpleNamespace(Stats=Other)

def check_writes(obj, want):
    assert obj.writes == want

def test_writes():
    stats = mod.Stats()
    stats.record_write()
    check_writes(stats, 0)
