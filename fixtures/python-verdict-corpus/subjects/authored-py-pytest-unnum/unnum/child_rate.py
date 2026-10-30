from typing import override


class Base:
    def rate(self, n):
        raise NotImplementedError


class Child(Base):
    @override
    def rate(self, n):
        return n + 1
