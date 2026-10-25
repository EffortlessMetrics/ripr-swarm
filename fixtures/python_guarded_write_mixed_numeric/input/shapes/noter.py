class Recorder:
    def __init__(self):
        self.last = None

    def note(self, kind):
        if kind <= 9007199254740992.0:
            self.last = "fault"
