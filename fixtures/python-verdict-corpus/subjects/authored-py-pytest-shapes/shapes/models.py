"""Dataclass, property, classmethod and staticmethod shapes."""

from dataclasses import dataclass, field


@dataclass
class Invoice:
    amount: int
    tax_rate: float = 0.2
    tags: list = field(default_factory=list)

    def total(self):
        return round(self.amount * (1 + self.tax_rate))

    @property
    def is_large(self):
        return self.amount >= 1000

    @classmethod
    def free(cls):
        return cls(0, 0.0)

    @staticmethod
    def parse_amount(text):
        return int(text.strip())
