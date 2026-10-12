from hypothesis import settings

# Derandomized so a mutant run is reproducible.
settings.register_profile("corpus", derandomize=True, database=None, max_examples=200)
settings.load_profile("corpus")
