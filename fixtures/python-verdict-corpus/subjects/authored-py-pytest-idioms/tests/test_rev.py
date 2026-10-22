from hypothesis import given, strategies as st

from idioms.rev import reverse, unreverse


@given(st.text())
def test_round_trip(text):
    assert unreverse(reverse(text)) == text
