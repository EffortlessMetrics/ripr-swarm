from dataclasses import dataclass, field


def boom():
    raise RuntimeError("class creation failed")


@dataclass
class Invoice:
    amount: int
    tax_rate: float = 0.25

    def total(self):
        return round(self.amount * (1 + self.tax_rate))

    async def other(self, extra=boom()):
        return 1
