from idioms.texts import shout


def test_shout_adds_an_exclamation():
    assert shout("hey") == "HEY!"
