<!-- section: Internal / CI -->
- Python verdict corpus: new `authored-py-pytest-shapes` subject and 31 cases
  labeled against real pytest runs (dataclasses, decorators, `asyncio.run`,
  comprehensions, dunder methods, `match`, relative imports). ripr scores them
  with 6 false actionable and 3 false exposed verdicts, each tracked under
  #9412 until its fix lands (#9413).
