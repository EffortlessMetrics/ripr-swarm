from unnum.child_rate import Child


def test_child_rate_adds_one():
    assert Child().rate(1) == 2
