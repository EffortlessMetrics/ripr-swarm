class numeric_range:
    def __init__(self, stop):
        self.stop = stop

    def __reversed__(self):
        if not self.stop:
            return iter(())
        return iter(range(self.stop - 1, -1, -1))

    def __len__(self):
        return self.stop
