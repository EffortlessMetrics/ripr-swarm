from ports import ranges

def test_hi_is_8080():
    assert "8080" in repr(ranges())
