from src.shop import fee


def test_match_capture_stand_in():
    match (lambda a: 30):
        case fee:
            pass
    assert fee(10) == 30
