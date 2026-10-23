class Stats:
    def __init__(self):
        self.writes = 0
        self.hits = 0

    async def record_write(self):
        self.writes += 1
