from hostile.dynamic import run


def test_run_add():
    assert run("add", 1, 2) == 3
