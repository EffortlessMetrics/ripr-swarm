class Recorder:
    def __init__(self):
        self.last = None

    def note(self, kind):
        if kind == "error":
            self.last = "fault"
        for self.last in ["settled"]:
            pass
