from shapes.comps import (
    countdown,
    distinct_pairs,
    index_by_name,
    squares_of_evens,
    total_lengths,
)


def test_squares_of_evens():
    assert squares_of_evens([1, 2, 3, 4]) == [4, 16]


def test_index_by_name():
    assert index_by_name([{"name": "a", "id": 1}]) == {"a": 1}


def test_total_lengths():
    assert total_lengths(["ab", "c"]) == 3


def test_distinct_pairs_counts_disjoint_inputs():
    assert len(distinct_pairs([1, 2], [3, 4])) == 4


def test_countdown_lists_every_step():
    assert list(countdown(3)) == [3, 2, 1]
