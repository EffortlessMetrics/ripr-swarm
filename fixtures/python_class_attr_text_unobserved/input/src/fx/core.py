class ValidationError(Exception):
    pass


class Length:
    message_max = "Longer than the maximum length {max}."

    def __init__(self, max=None):
        self.max = max

    def __call__(self, value):
        if self.max is not None and len(value) > self.max:
            raise ValidationError(self.message_max.format(max=self.max))
        return value
