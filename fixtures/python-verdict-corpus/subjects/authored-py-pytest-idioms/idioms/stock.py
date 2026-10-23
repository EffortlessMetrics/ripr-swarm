class Stock:
    def __init__(self, count):
        self.count = count

    def take(self, n):
        if n > self.count:
            raise ValueError("not enough stock")
        self.count -= n
        return self.count
