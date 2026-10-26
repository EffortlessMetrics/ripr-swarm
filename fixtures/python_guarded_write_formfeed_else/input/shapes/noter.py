class Recorder:
    def __init__(self):
        self.last = None

    def note(self, kind):
        if kind == "error":
            pass
        else:
            if kind == "error":
                self.last = "fault"
