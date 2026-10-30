class Recorder:
    def __init__(self):
        self.last=None
    def note(self, kind):
        other = Recorder()
        (kind), other.last = "ok", None
        if kind == "error":
            self.last = "fault"
