_cache = {}


def doubled(key):
    if key not in _cache:
        _cache[key] = key * 2
    return _cache[key]
