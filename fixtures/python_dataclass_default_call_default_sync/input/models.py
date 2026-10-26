from dataclasses import dataclass, field


@dataclass
class Invoice:
    amount: int
    tax_rate: float = 0.25

    def total(self):
        return round(self.amount * (1 + self.tax_rate))

    def other(self, extra=boom()):
        return 1
