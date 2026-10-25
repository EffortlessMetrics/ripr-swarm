class Money:
    def __init__(self, cents):
        self.cents = cents

    def __bool__(self):
        return self.cents > 0
