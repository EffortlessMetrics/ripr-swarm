def retry(times):
    def wrap(func):
        def run(*args, **kwargs):
            last = None
            for _ in range(times):
                try:
                    return func(*args, **kwargs)
                except RuntimeError as exc:
                    last = exc
            raise last

        return run

    return wrap


@retry(times=3)
def fetch_total(client):
    return client.get_total()
