class EmptyError(Exception): pass
class TooLongError(Exception): pass

def parse(s):
    if not s:
        raise EmptyError()
    if len(s) > 10:
        raise TooLongError()
    return len(s)
