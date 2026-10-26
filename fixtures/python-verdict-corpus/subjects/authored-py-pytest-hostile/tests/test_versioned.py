from hostile.versioned import versioned


def test_versioned():
    assert versioned() == "new"
