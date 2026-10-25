class Recorder:
    def __init__(self):
        self.last = None

    def note(self, kind):
        (kind) = "ok"
        if kind == "error":
            self.last = "fault"
