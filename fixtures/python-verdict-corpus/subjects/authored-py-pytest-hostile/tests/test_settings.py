from hostile.settings import over_limit


class TestLimit:
    def test_above(self):
        assert over_limit(11)

    def test_below(self):
        assert not over_limit(5)
