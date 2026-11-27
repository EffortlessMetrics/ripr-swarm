class Cart:
    def __init__(self, total):
        self.total = total

    async def discounted(self):
        if self.total >= 100:
            return self.total - 5
        return self.total
