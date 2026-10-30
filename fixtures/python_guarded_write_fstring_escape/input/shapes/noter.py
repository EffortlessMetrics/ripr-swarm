class Recorder:
    def __init__(self):
        self.last = None

    def note(self, kind):
        marker = f"{exit() if kind == 'error' else 0}"
        if kind == "error":
            self.last = "fault"
