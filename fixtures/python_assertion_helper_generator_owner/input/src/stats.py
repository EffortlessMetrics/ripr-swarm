class Stats:
    def __init__(self):
        self.writes = 0
        self.hits = 0

    def record_write(self):
        self.writes += 1
        f"{(yield None)}"
