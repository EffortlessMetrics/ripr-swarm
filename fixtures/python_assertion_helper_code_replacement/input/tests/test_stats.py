from src.stats import Stats
from tests.helpers import check_writes

def test_writes():
    stats = Stats()
    stats.record_write()
    check_writes(stats, 0)
