"""Side effects on counters, one method per test style."""


class Stats:
    def __init__(self):
        self.hits = 0
        self.misses = 0
        self.reads = 0
        self.writes = 0
        self.evictions = 0
        self.last_event = None

    def record_hit(self):
        self.hits = self.hits + 1
        self.last_event = "hits"

    def record_miss(self):
        self.misses = self.misses + 1
        self.last_event = "misses"

    def record_read(self):
        self.reads = self.reads + 1
        self.last_event = "reads"

    def record_write(self):
        self.writes = self.writes + 1
        self.last_event = "writes"

    def record_eviction(self):
        self.evictions = self.evictions + 1
        self.last_event = "evictions"
