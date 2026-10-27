<!-- section: Fixed -->
- Python: a default edit of a field in a plain `@dataclass` class is credited when a straight-line test assertion builds the class without that field and its method or attribute value holds with the old default and fails with the new one, instead of abstaining on `decorator_indirection` (#9619).
