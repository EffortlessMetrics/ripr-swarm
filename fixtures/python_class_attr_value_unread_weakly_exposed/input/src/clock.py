class Clock:
    _PER_YEAR = 13
    _PER_DAY = 24

    def __init__(self, hours):
        self.hours = hours

    def days(self):
        return self.hours // self._PER_DAY

    def years(self, months):
        return months // self._PER_YEAR
