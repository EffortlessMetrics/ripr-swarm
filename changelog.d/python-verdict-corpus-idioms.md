<!-- section: Internal / CI -->
- Python verdict corpus: new `authored-py-pytest-idioms` subject and 24 cases
  labeled against real pytest runs (`MagicMock`, `patch`, `monkeypatch`,
  doctests, `pytest.approx`, `pytest.raises(match=)`, skip/xfail, `caplog`,
  `parametrize`, a Hypothesis round trip, `unittest` setUp, autouse fixtures).
  ripr scores them with 3 false actionable and 1 false exposed verdict, each
  tracked under #9412 until its fix lands (#9413).
