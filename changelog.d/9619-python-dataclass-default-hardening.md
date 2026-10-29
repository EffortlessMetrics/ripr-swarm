<!-- section: Fixed -->
- Python: a plain `@dataclass` default credit now also requires that the class body runs no unmodelled code (non-builtin `default_factory`, call defaults, unknown method decorators, non-builtin annotations) and that the test takes no fixture parameters (#9619).
