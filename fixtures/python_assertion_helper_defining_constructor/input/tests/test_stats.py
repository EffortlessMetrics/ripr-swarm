from tests.helpers import check_writes
from src.stats import Stats

def test_writes():
    stats = Stats()
    stats.record_write()
    check_writes(stats, 0)
