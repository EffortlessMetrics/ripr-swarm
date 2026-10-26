def poke(module, name, key, value):
    getattr(module, name)[key] = value
