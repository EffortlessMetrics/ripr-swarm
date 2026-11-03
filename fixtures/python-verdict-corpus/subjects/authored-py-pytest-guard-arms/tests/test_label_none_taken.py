from guards.label_none_taken import label_none_taken


def test_label_of_none():
    assert label_none_taken(None) == 'none'
