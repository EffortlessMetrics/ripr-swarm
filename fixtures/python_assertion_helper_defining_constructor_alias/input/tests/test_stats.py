from tests.helpers import check_writes
from src.stats import Stats as Counter

def test_writes():
    stats = Counter()
    stats.record_write()
    check_writes(stats, 0)
