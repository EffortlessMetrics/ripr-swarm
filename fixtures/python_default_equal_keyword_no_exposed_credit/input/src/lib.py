class Clock:
    def __init__(self, t):
        self.t = t

    def shift(self, check_imaginary=True, **kwargs):
        return Clock(self.t + kwargs.get("hours", 0))


def dehumanize(current_time, **time_changes):
    return current_time.shift(check_imaginary=True, **time_changes)
