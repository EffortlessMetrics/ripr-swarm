from routeclients.mini import TestClient
from routeclients.reports import build_app


def test_reports_respond():
    client = TestClient(build_app())
    assert client.get("/reports").json() is not None
