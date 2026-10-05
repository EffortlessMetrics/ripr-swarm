from hypothesis import assume, example, given
from hypothesis import strategies as st

from stats import absolute, clamp, grade_label, is_passing, mean


@given(st.integers(), st.integers(), st.integers())
def test_clamp_stays_in_range(x, lo, hi):
    assume(lo <= hi)
    assert lo <= clamp(x, lo, hi) <= hi


@given(st.integers())
def test_absolute_matches_builtin(x):
    assert absolute(x) == abs(x)


@given(st.lists(st.integers(-1000, 1000), min_size=1))
def test_mean_lies_between_extremes(xs):
    assert min(xs) <= mean(xs) <= max(xs)


@given(st.integers(0, 100))
@example(49)
@example(50)
def test_passing_threshold(score):
    assert is_passing(score) == (score >= 50)


@given(st.integers(0, 100))
def test_label_never_crashes(score):
    grade_label(score)
