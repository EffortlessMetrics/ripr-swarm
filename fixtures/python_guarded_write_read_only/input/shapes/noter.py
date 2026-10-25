class Recorder:
    def __init__(self):
        self.last = None

    def note(self, kind):
        if kind == "error":  # plain guard
            self.last = "fault"
        if self.last:
            marker = {self.last: 1}
        self.last: str
