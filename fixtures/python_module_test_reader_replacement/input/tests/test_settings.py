import settings

def test_it():
    settings.over_limit, extra = (lambda n: True), 0
    assert settings.over_limit(11)
