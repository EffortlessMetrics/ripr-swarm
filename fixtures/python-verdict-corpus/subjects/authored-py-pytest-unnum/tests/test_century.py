from unnum.century import century_index


def test_century_index_is_computed():
    assert century_index(1999) is not None
