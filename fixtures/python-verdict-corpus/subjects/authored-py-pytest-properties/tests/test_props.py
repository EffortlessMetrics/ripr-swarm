from props.core import clamp_a, clamp_b, clamp_c, clamp_h, scale_d, scale_e, scale_j


def test_clamp_a():
    assert clamp_a(clamp_a(-3)) == clamp_a(-3)


def test_clamp_h():
    assert clamp_h(-3) == clamp_h(clamp_h(-3))


def test_clamp_b():
    assert clamp_b(clamp_b(-3)) == clamp_b(-3) == 0


def test_clamp_c():
    assert clamp_c(-3) == 0


def test_scale_d():
    for n in range(1, 5):
        assert scale_d(1, n) == n


def test_scale_j():
    for i, n in enumerate(range(1, 5)):
        assert scale_j(1, n) == n


def test_scale_e():
    expected = 6
    assert scale_e(2, 3) == expected
