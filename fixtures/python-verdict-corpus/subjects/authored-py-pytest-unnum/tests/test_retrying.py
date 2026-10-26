from unnum.retrying import fetch_total


class FakeClient:
    def get_total(self):
        return 42


def test_fetch_total_returns_the_client_total():
    assert fetch_total(FakeClient()) == 42
