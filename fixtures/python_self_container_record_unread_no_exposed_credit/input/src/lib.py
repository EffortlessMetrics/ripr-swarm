class Clock:
    def __init__(self, hours):
        self.hours = hours
        self._seen = []

    def tick(self, n):
        self._seen.append(n + 1)
        return self.hours + n

    def count(self):
        return len(self._seen)

    def total(self):
        return self.count() * 2
