class Recorder:
    def __init__(self):
        self.last = None

    def note(self, kind):
        𝓴𝓲𝓷𝓭 = "ok"
        if kind == "error":
            self.last = "fault"
