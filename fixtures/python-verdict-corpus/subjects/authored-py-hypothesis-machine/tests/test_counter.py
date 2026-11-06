from hypothesis import settings
from hypothesis.stateful import RuleBasedStateMachine, invariant, rule
from counter import increment

class CounterMachine(RuleBasedStateMachine):
    def __init__(self):
        super().__init__()
        self.actual = 0
        self.expected = 0

    @rule()
    def advance(self):
        self.actual = increment(self.actual)
        self.expected += 1

    @invariant()
    def agrees(self):
        assert self.actual == self.expected

TestCounter = CounterMachine.TestCase
TestCounter.settings = settings(
    max_examples=5, stateful_step_count=3,
    database=None, derandomize=True,
)
