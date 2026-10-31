from derived.core import make_input, scale_input


def test_scale():
    n = make_input()
    assert scale_input(n) == n * 2
