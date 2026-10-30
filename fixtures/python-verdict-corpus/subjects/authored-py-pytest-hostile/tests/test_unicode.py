from hostile.unicode_names import größe, grüße


def test_größe_doubles():
    assert größe(2) == 4


def test_grüße_greets():
    assert grüße("Ann") == "Grüß Ann"
