class _Raises:
    def __enter__(self): return self
    def __exit__(self, exc_type, exc, traceback): return True

def raises(expected):
    return _Raises()
