from reach.prepared import post


def test_post_encodes_body():
    assert post("ok") == "OK"
