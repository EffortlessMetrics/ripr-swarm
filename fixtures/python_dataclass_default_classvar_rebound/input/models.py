from dataclasses import dataclass, field

float = ClassVar


@dataclass
class Invoice:
    amount: int
    tax_rate: float = 0.25

    def total(self):
        return round(self.amount * (1 + self.tax_rate))
