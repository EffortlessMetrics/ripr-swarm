class Recorder:
    def __init__(self):
        self.last = None

    def note(self, kind):
        if kind == "error":  # plain guard
            self.last = "fault"
