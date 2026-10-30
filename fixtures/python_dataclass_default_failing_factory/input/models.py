from dataclasses import dataclass, field


def fail():
    raise RuntimeError("factory failed")


@dataclass
class Invoice:
    amount: int
    tax_rate: float = 0.25
    tags: list = field(default_factory=fail)

    def total(self):
        return round(self.amount * (1 + self.tax_rate))
