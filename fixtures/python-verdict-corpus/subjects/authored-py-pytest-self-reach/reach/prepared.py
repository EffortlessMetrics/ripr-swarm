class PreparedRequest:
    def _encode(self, body):
        return body.upper()

    def prepare(self, body):
        return self._encode(body)


def post(body):
    return PreparedRequest().prepare(body)
