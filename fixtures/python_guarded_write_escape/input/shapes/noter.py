class Recorder:
    def __init__(self):
        self.last = None

    def note(self, kind):
        if kind == "error":
            marker = "\""; return
        if kind == "error":
            self.last = "fault"
