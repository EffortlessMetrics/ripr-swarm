from dataclasses import dataclass, field


def tweak(fn):
    return None


@dataclass
class Invoice:
    amount: int
    tax_rate: float = 0.25

    def total(self):
        return round(self.amount * (1 + self.tax_rate))

    @tweak
    def other(self):
        return 1
