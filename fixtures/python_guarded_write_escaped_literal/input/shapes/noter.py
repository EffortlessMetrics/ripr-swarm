class Recorder:
    def __init__(self):
        self.last = None

    def note(self, kind):
        if kind != "\x65rror":
            self.last = "fault"
